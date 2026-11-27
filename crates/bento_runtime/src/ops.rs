use std::borrow::Cow;
use std::sync::Arc;

use deno_core::{OpDecl, OpState, op2};
use deno_error::JsErrorBox;

use bento_host::Permission;

use crate::environment::{
    InstanceEnvironment, bento_cwd, bento_delete_env, bento_env, bento_get_env, bento_set_env,
    op_bento_own_environment, op_bento_register_env_warning,
};
use crate::host::HostModules;
use crate::host_ops::op_bento_host_call;
use crate::lifecycle::Control;
use crate::permissions::Authority;
use crate::signals::{bento_signal_bind, bento_signal_poll, bento_signal_unbind};
use crate::watch_inputs::{WatchInputs, op_bento_failed_require, op_bento_missing_require_target};

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
/// JavaScript removes the dropped variables from the isolate's own `process.env`, which in a
/// worker is a copy.
#[op2]
fn op_bento_permission_drop(
    state: &OpState,
    #[string] scope: &str,
    #[string] reference: Option<String>,
) {
    let authority = state.borrow::<Arc<Authority>>();
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
            }
            (permission, _) => authority.give_up(|permissions| permissions.drop_whole(permission)),
        }
    }
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
            .or_else(|| crate::require_ops::replacement(original.name))
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
        op_bento_own_environment,
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
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/permission/child_process.mjs",
            deno_core::ascii_str_include!("permission/child_process.mjs"),
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/permission/errors.mjs",
            deno_core::ascii_str_include!("permission/errors.mjs"),
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/permission/file_system.mjs",
            deno_core::ascii_str_include!("permission/file_system.mjs"),
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/permission/network.mjs",
            deno_core::ascii_str_include!("permission/network.mjs"),
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/permission/process_permission.mjs",
            deno_core::ascii_str_include!("permission/process_permission.mjs"),
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/permission/wasi_inspector.mjs",
            deno_core::ascii_str_include!("permission/wasi_inspector.mjs"),
        ), deno_core::ExtensionFileSource::new(
            "ext:bento_instance/worker_environment.mjs",
            deno_core::ascii_str_include!("worker_environment.mjs"),
        )];
        extension.esm_files = Cow::Borrowed(HOST_SOURCE);
    },
);
