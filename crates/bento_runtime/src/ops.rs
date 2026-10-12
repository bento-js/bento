use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use deno_core::{OpDecl, OpState, op2};
use deno_error::JsErrorBox;
use parking_lot::Mutex;

use crate::host::HostModules;
use crate::host_ops::op_bento_host_call;
use crate::lifecycle::Control;
use crate::signals::{bento_signal_bind, bento_signal_poll, bento_signal_unbind};
use crate::watch_inputs::{WatchInputs, op_bento_failed_require, op_bento_missing_require_target};
use bento_host::Scope;

#[derive(Clone)]
pub(crate) struct InstanceEnvironment {
    pub values: Arc<Mutex<BTreeMap<String, String>>>,
    pub allowed: Scope<String>,
    pub cwd: PathBuf,
}

#[op2]
#[string]
fn bento_get_env(state: &OpState, #[string] key: &str) -> Option<String> {
    let environment = state.borrow::<InstanceEnvironment>();
    environment.allowed.allows(key).then(|| environment.values.lock().get(key).cloned()).flatten()
}

#[op2]
#[serde]
fn bento_env(state: &OpState) -> BTreeMap<String, String> {
    let environment = state.borrow::<InstanceEnvironment>();
    environment
        .values
        .lock()
        .iter()
        .filter(|(key, _)| environment.allowed.allows(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[op2(fast)]
fn bento_set_env(
    state: &mut OpState,
    #[string] key: &str,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    let environment = state.borrow_mut::<InstanceEnvironment>();
    check_key(environment, key)?;
    if value.contains('\0') {
        return Err(JsErrorBox::type_error("environment values cannot contain NUL"));
    }
    environment.values.lock().insert(key.to_owned(), value.to_owned());
    Ok(())
}

#[op2(fast)]
fn bento_delete_env(state: &mut OpState, #[string] key: &str) -> Result<(), JsErrorBox> {
    let environment = state.borrow_mut::<InstanceEnvironment>();
    check_key(environment, key)?;
    environment.values.lock().remove(key);
    Ok(())
}

fn check_key(environment: &InstanceEnvironment, key: &str) -> Result<(), JsErrorBox> {
    if !environment.allowed.allows(key) {
        return Err(JsErrorBox::new("NotCapable", format!("Requires env access to {key}")));
    }
    if key.is_empty() || key.contains(['=', '\0']) {
        return Err(JsErrorBox::type_error("invalid environment variable name"));
    }
    Ok(())
}

#[op2]
#[string]
fn bento_cwd(state: &OpState) -> String {
    state.borrow::<InstanceEnvironment>().cwd.to_string_lossy().into_owned()
}

#[op2(fast)]
fn bento_deny_process_mutation(#[string] _path: &str) -> Result<(), JsErrorBox> {
    Err(JsErrorBox::new("NotCapable", "Changing host process state is not permitted"))
}
#[op2(fast)]
fn bento_exit(state: &mut OpState) {
    state.put(deno_runtime::deno_os::WatcherExited);
    state.borrow::<deno_runtime::deno_os::WatcherExitHandle>().0.terminate_execution();
    if !state.has::<crate::workers::WorkerIsolate>() {
        state.borrow::<Arc<Control>>().dispose();
    }
}

#[op2(fast)]
fn bento_deny_host_termination() -> Result<(), JsErrorBox> {
    Err(JsErrorBox::new(
        "NotSupported",
        "Host process termination and signals are controlled by the host",
    ))
}

#[op2(fast)]
fn bento_fixed_permissions() -> Result<(), JsErrorBox> {
    Err(JsErrorBox::new("NotSupported", "Permission grants are fixed by the host"))
}

fn middleware(original: OpDecl) -> OpDecl {
    let mut replacement = match original.name {
        "op_get_env" | "op_get_env_no_permission_check" => bento_get_env(),
        "op_env" => bento_env(),
        "op_set_env" => bento_set_env(),
        "op_delete_env" => bento_delete_env(),
        "op_fs_cwd" => bento_cwd(),
        "op_fs_chdir" | "op_node_load_env_file" => bento_deny_process_mutation(),
        "op_exit" => bento_exit(),
        "op_process_abort" | "op_node_process_kill" | "op_kill" => bento_deny_host_termination(),
        "op_request_permission" | "op_revoke_permission" => bento_fixed_permissions(),
        "op_signal_bind" => bento_signal_bind(),
        "op_signal_poll" => bento_signal_poll(),
        "op_signal_unbind" => bento_signal_unbind(),
        _ => match crate::feature_ops::replacement(original.name)
            .or_else(|| crate::fs_ops::replacement(original.name))
        {
            Some(replacement) => replacement,
            None => return original,
        },
    };
    replacement.name = original.name;
    replacement.name_fast = original.name_fast;
    replacement
}

deno_core::extension!(bento_instance,
    ops = [op_bento_host_call, op_bento_failed_require, op_bento_missing_require_target],
    esm_entry_point = "ext:bento_instance/host.mjs",
    options = { environment: InstanceEnvironment, modules: HostModules, control: Arc<Control>, watch_inputs: WatchInputs },
    middleware = middleware,
    state = |state, options| {
        state.put(options.environment);
        state.put(options.modules);
        state.put(options.control);
        state.put(options.watch_inputs);
    },
    customizer = |extension: &mut deno_core::Extension| {
        const HOST_SOURCE: &[deno_core::ExtensionFileSource] = &[deno_core::ExtensionFileSource::new(
            "ext:bento_instance/host.mjs",
            deno_core::ascii_str_include!("host.mjs"),
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/require_watch.mjs",
            deno_core::ascii_str_include!("require_watch.mjs"),
        )];
        extension.esm_files = Cow::Borrowed(HOST_SOURCE);
    },
);
