use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use deno_core::{Extension, OpDecl, OpState, op2, v8};
use deno_error::JsErrorBox;
use deno_runtime::ops::worker_host::{CreateWebWorkerCb, deno_worker_host};

struct OriginalFeatureOps {
    worker: v8::Global<v8::Function>,
    tcp: v8::Global<v8::Function>,
    tls: v8::Global<v8::Function>,
}

pub(crate) fn extension(callback: Arc<CreateWebWorkerCb>) -> Extension {
    // FeatureChecker's callback cannot stop its caller; preflight the preserved native ops.
    let mut extension = bento_feature_ops::init();
    let worker = deno_worker_host::init(callback, None)
        .ops
        .iter()
        .find(|op| op.name == "op_create_worker")
        .copied()
        .expect("upstream worker creation op exists");
    let mut worker = worker;
    worker.name = "op_bento_original_worker";
    worker.name_fast = deno_core::ascii_str!("op_bento_original_worker");
    let mut tcp = deno_runtime::deno_net::ops::op_net_listen_tcp();
    tcp.name = "op_bento_original_tcp";
    tcp.name_fast = deno_core::ascii_str!("op_bento_original_tcp");
    let mut tls = deno_runtime::deno_net::ops_tls::op_net_listen_tls();
    tls.name = "op_bento_original_tls";
    tls.name_fast = deno_core::ascii_str!("op_bento_original_tls");
    extension.ops.to_mut().extend([worker, tcp, tls]);
    extension
}

pub(crate) fn replacement(name: &str) -> Option<OpDecl> {
    Some(match name {
        "op_create_worker" => bento_create_worker(),
        "op_net_listen_tcp" => bento_listen_tcp(),
        "op_net_listen_tls" => bento_listen_tls(),
        "op_cron_create"
        | "op_kv_database_open"
        | "op_net_listen_udp"
        | "op_net_listen_unixpacket"
        | "op_net_connect_vsock"
        | "op_net_listen_vsock" => bento_deny_feature(),
        _ => return None,
    })
}

fn unsupported(api: &str) -> JsErrorBox {
    JsErrorBox::new("NotSupported", format!("{api} requires an unsupported runtime feature"))
}

#[op2(fast, stack_trace)]
fn bento_deny_feature() -> Result<(), JsErrorBox> {
    Err(unsupported("This operation"))
}

#[op2(nofast)]
fn op_bento_register_feature_ops(
    scope: &mut v8::PinScope,
    state: &mut OpState,
    worker: v8::Local<v8::Function>,
    tcp: v8::Local<v8::Function>,
    tls: v8::Local<v8::Function>,
) {
    state.put(OriginalFeatureOps {
        worker: v8::Global::new(scope, worker),
        tcp: v8::Global::new(scope, tcp),
        tls: v8::Global::new(scope, tls),
    });
}

fn property<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    name: &str,
) -> Option<v8::Local<'s, v8::Value>> {
    let key = v8::String::new(scope, name)?;
    object.get(scope, key.into())
}

fn invoke<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    original: v8::Local<'s, v8::Function>,
    arguments: &[v8::Local<'s, v8::Value>],
) -> Option<v8::Local<'s, v8::Value>> {
    let receiver = v8::undefined(scope).into();
    original.call(scope, receiver, arguments)
}

#[op2(reentrant, stack_trace)]
fn bento_create_worker<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: Rc<RefCell<OpState>>,
    arguments: v8::Local<'s, v8::Object>,
    metadata: v8::Local<'s, v8::Value>,
) -> Result<Option<v8::Local<'s, v8::Value>>, JsErrorBox> {
    let Some(permissions) = property(scope, arguments, "permissions") else { return Ok(None) };
    if !permissions.is_null_or_undefined() {
        return Err(unsupported("Worker.deno.permissions"));
    }
    let original = v8::Local::new(scope, &state.borrow().borrow::<OriginalFeatureOps>().worker);
    Ok(invoke(scope, original, &[arguments.into(), metadata]))
}

#[op2(reentrant, stack_trace)]
fn bento_listen_tcp<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: Rc<RefCell<OpState>>,
    address: v8::Local<'s, v8::Value>,
    reuse_port: bool,
    load_balanced: v8::Local<'s, v8::Value>,
    backlog: v8::Local<'s, v8::Value>,
) -> Result<Option<v8::Local<'s, v8::Value>>, JsErrorBox> {
    if reuse_port {
        return Err(unsupported("Deno.listen({ reusePort: true })"));
    }
    let original = v8::Local::new(scope, &state.borrow().borrow::<OriginalFeatureOps>().tcp);
    let reuse_port = v8::Boolean::new(scope, false).into();
    Ok(invoke(scope, original, &[address, reuse_port, load_balanced, backlog]))
}

#[op2(reentrant, stack_trace)]
fn bento_listen_tls<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: Rc<RefCell<OpState>>,
    address: v8::Local<'s, v8::Value>,
    arguments: v8::Local<'s, v8::Object>,
    keys: v8::Local<'s, v8::Value>,
) -> Result<Option<v8::Local<'s, v8::Value>>, JsErrorBox> {
    let Some(reuse_port) = property(scope, arguments, "reusePort") else { return Ok(None) };
    if reuse_port.boolean_value(scope) {
        return Err(unsupported("Deno.listenTls({ reusePort: true })"));
    }
    let original = v8::Local::new(scope, &state.borrow().borrow::<OriginalFeatureOps>().tls);
    Ok(invoke(scope, original, &[address, arguments.into(), keys]))
}

deno_core::extension!(bento_feature_ops,
    ops = [op_bento_register_feature_ops],
    esm_entry_point = "ext:bento_feature_ops/feature_ops.mjs",
    esm = [dir "src", "feature_ops.mjs"],
);
