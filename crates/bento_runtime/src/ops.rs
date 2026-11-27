use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use deno_core::{OpDecl, OpState, op2, v8};
use deno_error::JsErrorBox;
use parking_lot::Mutex;

use bento_host::{Permission, Permissions, is_always_kept};

use crate::host::HostModules;
use crate::host_ops::op_bento_host_call;
use crate::lifecycle::Control;
use crate::permissions::Authority;
use crate::signals::{bento_signal_bind, bento_signal_poll, bento_signal_unbind};
use crate::watch_inputs::{WatchInputs, op_bento_failed_require, op_bento_missing_require_target};

/// The environment every isolate of one instance reads and writes.
#[derive(Clone)]
pub(crate) struct InstanceEnvironment {
    pub values: Arc<Mutex<BTreeMap<String, String>>>,
    /// Variables removed before guest code ran whose first read has not been warned about.
    unwarned: Arc<Mutex<BTreeSet<String>>>,
    pub cwd: PathBuf,
}

impl InstanceEnvironment {
    /// The environment an instance starts with: `values`, less every variable `permissions`
    /// removes unless the host keeps it in `kept`.
    pub(crate) fn new(
        mut values: BTreeMap<String, String>,
        kept: &BTreeSet<String>,
        permissions: &Permissions,
        cwd: PathBuf,
    ) -> Self {
        let mut removed = BTreeSet::new();
        if permissions.withholds_anything() {
            values.retain(|name, _| {
                let keep = kept.contains(name) || !permissions.removes_variable(name);
                if !keep {
                    removed.insert(name.clone());
                }
                keep
            });
        }
        Self { values: Arc::new(Mutex::new(values)), unwarned: Arc::new(Mutex::new(removed)), cwd }
    }
}

/// How an isolate emits the warning for the first read of a removed variable.
struct RemovedVariableWarning(v8::Global<v8::Function>);

#[op2(nofast)]
fn op_bento_register_env_warning(
    scope: &mut v8::PinScope,
    state: &mut OpState,
    warn: v8::Local<v8::Function>,
) {
    state.put(RemovedVariableWarning(v8::Global::new(scope, warn)));
}

#[op2(reentrant)]
#[string]
fn bento_get_env(
    scope: &mut v8::PinScope,
    state: Rc<RefCell<OpState>>,
    #[string] key: String,
) -> Option<String> {
    let (value, warn) = {
        let state = state.borrow();
        let environment = state.borrow::<InstanceEnvironment>();
        let value = environment.values.lock().get(&key).cloned();
        let first_read = value.is_none() && environment.unwarned.lock().remove(&key);
        let warn = first_read
            .then(|| state.try_borrow::<RemovedVariableWarning>().map(|warning| warning.0.clone()))
            .flatten();
        (value, warn)
    };
    if let Some(warn) = warn {
        let warn = v8::Local::new(scope, warn);
        let receiver = v8::undefined(scope).into();
        let name = v8::String::new(scope, &key).map(Into::into);
        if let Some(name) = name {
            warn.call(scope, receiver, &[name]);
        }
    }
    value
}

#[op2]
#[serde]
fn bento_env(state: &OpState) -> BTreeMap<String, String> {
    state.borrow::<InstanceEnvironment>().values.lock().clone()
}

#[op2(fast)]
fn bento_set_env(
    state: &mut OpState,
    #[string] key: &str,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    check_key(key)?;
    if value.contains('\0') {
        return Err(JsErrorBox::type_error("environment values cannot contain NUL"));
    }
    let environment = state.borrow::<InstanceEnvironment>();
    environment.unwarned.lock().remove(key);
    environment.values.lock().insert(key.to_owned(), value.to_owned());
    Ok(())
}

#[op2(fast)]
fn bento_delete_env(state: &mut OpState, #[string] key: &str) -> Result<(), JsErrorBox> {
    check_key(key)?;
    state.borrow::<InstanceEnvironment>().values.lock().remove(key);
    Ok(())
}

fn check_key(key: &str) -> Result<(), JsErrorBox> {
    if key.is_empty() || key.contains(['=', '\0']) {
        return Err(JsErrorBox::type_error("invalid environment variable name"));
    }
    Ok(())
}

/// Whether the instance withholds anything, which is when `process.permission` exists.
#[op2(fast)]
fn op_bento_permission_withholds(state: &OpState) -> bool {
    state.borrow::<Arc<Authority>>().current().withholds_anything()
}

/// `process.permission.has(scope, reference)`, after JavaScript has validated its arguments
/// and turned a URL or byte reference into a path.
#[op2]
fn op_bento_permission_has(
    state: &OpState,
    #[string] scope: &str,
    #[string] reference: Option<String>,
) -> bool {
    let authority = state.borrow::<Arc<Authority>>();
    let permissions = authority.current();
    let file = |permission| match &reference {
        Some(reference) => authority
            .locate(reference)
            .is_some_and(|path| permissions.holds_path(permission, &path)),
        None => permissions.holds(permission),
    };
    match scope {
        "fs" => file(Permission::FsRead) && file(Permission::FsWrite),
        _ => match Permission::from_scope(scope) {
            Some(permission) if permission.is_file_system() => file(permission),
            Some(Permission::Env) => match &reference {
                Some(name) => permissions.has_variable(name),
                None => permissions.holds(Permission::Env),
            },
            Some(permission) => permissions.holds(permission),
            None => false,
        },
    }
}

/// `process.permission.drop(scope, reference)`, after JavaScript has validated its arguments.
#[op2]
fn op_bento_permission_drop(
    state: &OpState,
    #[string] scope: &str,
    #[string] reference: Option<String>,
) {
    let authority = state.borrow::<Arc<Authority>>();
    let environment = state.borrow::<InstanceEnvironment>();
    let permissions = match scope {
        "fs" => vec![Permission::FsRead, Permission::FsWrite],
        _ => Permission::from_scope(scope).into_iter().collect(),
    };
    for permission in permissions {
        match (permission, &reference) {
            (Permission::FsRead | Permission::FsWrite, Some(reference)) => {
                if let Some(pattern) = authority.pattern(reference) {
                    authority.give_up(|permissions| permissions.drop_pattern(permission, &pattern));
                }
            }
            (Permission::Env, Some(name)) => {
                authority.give_up(|permissions| permissions.drop_variable(name));
                environment.values.lock().remove(name);
            }
            (Permission::Env, None) => {
                authority.give_up(|permissions| permissions.drop_whole(Permission::Env));
                environment.values.lock().retain(|name, _| is_always_kept(name));
            }
            (permission, _) => authority.give_up(|permissions| permissions.drop_whole(permission)),
        }
    }
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
    Err(JsErrorBox::new("NotSupported", "Permissions are fixed by the host"))
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
            .or_else(|| crate::subprocess_ops::replacement(original.name))
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
    ops = [
        op_bento_host_call,
        op_bento_failed_require,
        op_bento_missing_require_target,
        op_bento_register_env_warning,
        op_bento_permission_withholds,
        op_bento_permission_has,
        op_bento_permission_drop,
    ],
    esm_entry_point = "ext:bento_instance/host.mjs",
    options = {
        environment: InstanceEnvironment,
        authority: Arc<Authority>,
        modules: HostModules,
        control: Arc<Control>,
        watch_inputs: WatchInputs,
    },
    middleware = middleware,
    state = |state, options| {
        state.put(options.environment);
        state.put(options.authority);
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
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/permission.mjs",
            deno_core::ascii_str_include!("permission.mjs"),
        )];
        extension.esm_files = Cow::Borrowed(HOST_SOURCE);
    },
);
