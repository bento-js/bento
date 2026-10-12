import { core } from "ext:core/mod.js";
import "ext:bento_instance/require_watch.mjs";

let invokeHost = core.ops.op_bento_host_call;

Object.defineProperty(globalThis, Symbol.for("bento.host.invoke"), {
    value: async (specifier, name, arguments_) => {
        let response = JSON.parse(await invokeHost(specifier, name, arguments_));
        if (response.error !== null) {
            let error = new Error(response.error.message);
            error.name = response.error.name;
            throw error;
        }
        return response.value;
    },
});
