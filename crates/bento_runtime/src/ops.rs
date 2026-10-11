use std::collections::BTreeMap;
use std::path::PathBuf;

use deno_core::{OpDecl, OpState, op2};
use deno_error::JsErrorBox;

use bento_host::Scope;

pub(crate) struct InstanceEnvironment {
    pub values: BTreeMap<String, String>,
    pub allowed: Scope<String>,
    pub cwd: PathBuf,
}

#[op2]
#[string]
fn bento_get_env(state: &OpState, #[string] key: &str) -> Option<String> {
    let environment = state.borrow::<InstanceEnvironment>();
    environment.allowed.allows(key).then(|| environment.values.get(key).cloned()).flatten()
}

#[op2]
#[serde]
fn bento_env(state: &OpState) -> BTreeMap<String, String> {
    let environment = state.borrow::<InstanceEnvironment>();
    environment
        .values
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
    environment.values.insert(key.to_owned(), value.to_owned());
    Ok(())
}

#[op2(fast)]
fn bento_delete_env(state: &mut OpState, #[string] key: &str) -> Result<(), JsErrorBox> {
    let environment = state.borrow_mut::<InstanceEnvironment>();
    check_key(environment, key)?;
    environment.values.remove(key);
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

fn middleware(original: OpDecl) -> OpDecl {
    let mut replacement = match original.name {
        "op_get_env" | "op_get_env_no_permission_check" => bento_get_env(),
        "op_env" => bento_env(),
        "op_set_env" => bento_set_env(),
        "op_delete_env" => bento_delete_env(),
        "op_fs_cwd" => bento_cwd(),
        "op_fs_chdir" | "op_node_load_env_file" => bento_deny_process_mutation(),
        _ => return original,
    };
    replacement.name = original.name;
    replacement.name_fast = original.name_fast;
    replacement
}

deno_core::extension!(bento_instance,
    options = { environment: InstanceEnvironment },
    middleware = middleware,
    state = |state, options| state.put(options.environment),
);
