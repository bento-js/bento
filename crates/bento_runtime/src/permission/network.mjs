// Listeners, Unix-domain and UDP sockets, and DNS.
import {
    DNS_REFUSED,
    netDenied,
    nodeProperties,
    socketDenial,
} from "ext:bento_instance/permission/errors.mjs";
import { held } from "ext:bento_instance/permission/process_permission.mjs";
import { core, primordials } from "ext:core/mod.js";

let { Error, ObjectGetOwnPropertyNames, ReflectApply, StringPrototypeStartsWith } = primordials;

let nodeErrors = () => core.loadExtScript("ext:deno_node/internal/errors.ts");

/**
 * Must run before anything loads `node:net` or `node:dgram`, which keep the
 * `exceptionWithHostPort` they find when they load.
 */
export function guardNetwork() {
    guardSockets();
    guardListeners();
    guardResolvers();
}

/**
 * Node's socket bindings hand a refusal back where they return an error number, and its
 * `exceptionWithHostPort` quotes the denial; Deno's binding objects are patched at the same
 * points, so a refused Unix-domain connection or UDP bind reaches an `error` event.
 */
function guardSockets() {
    let errors = nodeErrors();
    let original = errors.exceptionWithHostPort;
    let quoting = function exceptionWithHostPort(err, syscall, address, port) {
        if (err?.code !== "ERR_ACCESS_DENIED") {
            return ReflectApply(original, this, arguments);
        }
        let { message, properties } = socketDenial(err, syscall, address, port);
        let error = new Error(message);
        nodeProperties(error, properties);
        return error;
    };
    errors.exceptionWithHostPort = quoting;
    errors.default.exceptionWithHostPort = quoting;

    let { Pipe } = core.loadExtScript("ext:deno_node/internal_binding/pipe_wrap.ts");
    refuseOnHandle(Pipe.prototype, "connect", 1);
    let { UDP } = core.loadExtScript("ext:deno_node/internal_binding/udp_wrap.ts");
    refuseOnHandle(UDP.prototype, "bind", 0);
    refuseOnHandle(UDP.prototype, "bind6", 0);
}

/** Makes `prototype[name]` return Node's denial, naming its argument at `address`. */
function refuseOnHandle(prototype, name, address) {
    let original = prototype[name];
    prototype[name] = function (...args) {
        return held("net") ? ReflectApply(original, this, args) : netDenied(args[address]);
    };
}

/**
 * Node refuses `server.listen()` itself, while Deno's Node layer reports the refusal as a
 * failed connection; check first so a listener is refused as Node refuses it, naming the
 * path of a Unix-domain socket.
 */
function guardListeners() {
    let { Server } = core.createLazyLoader("node:net")().default;
    let listen = Server.prototype.listen;
    Server.prototype.listen = function (...args) {
        if (!held("net")) throw netDenied(socketPath(args[0]) ?? "");
        return ReflectApply(listen, this, args);
    };
}

function socketPath(first) {
    if (typeof first === "string") return Number.isNaN(Number(first)) ? first : undefined;
    return typeof first?.path === "string" ? first.path : undefined;
}

/**
 * Node's DNS binding refuses a lookup by returning its code, completes a refused reverse
 * lookup at once, and completes a refused query on a later turn. Deno's binding is shaped
 * like Node's, so each request reaches the same callback or promise with Node's error.
 */
function guardResolvers() {
    let cares = core.loadExtScript("ext:deno_node/internal_binding/cares_wrap.ts");
    let binding = cares.default;
    let { getaddrinfo, getnameinfo } = binding;
    binding.getaddrinfo = function () {
        return held("net") ? ReflectApply(getaddrinfo, this, arguments) : DNS_REFUSED;
    };
    binding.getnameinfo = function (request, address) {
        if (held("net")) return ReflectApply(getnameinfo, this, arguments);
        // Node's promise API names no host in this error; its callback API does.
        let host = request.callback === undefined ? undefined : address;
        request.oncomplete(nodeErrors().dnsException(DNS_REFUSED, "getnameinfo", host));
        return 0;
    };
    let { setImmediate } = core.loadExtScript("ext:deno_node/timers.ts");
    let { prototype } = cares.ChannelWrap;
    for (let name of ObjectGetOwnPropertyNames(prototype)) {
        if (!StringPrototypeStartsWith(name, "query") && name !== "getHostByAddr") continue;
        let query = prototype[name];
        prototype[name] = function (request) {
            if (held("net")) return ReflectApply(query, this, arguments);
            setImmediate(() => request.oncomplete(DNS_REFUSED, []));
            return 0;
        };
    }
}
