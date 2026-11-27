use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use deno_core::{OpState, op2, v8};
use deno_error::JsErrorBox;
use parking_lot::Mutex;

use bento_host::Permissions;

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
    /// removes. The host keeps the variables in `kept` that the permissions merely do not
    /// allow; a variable they deny is removed all the same.
    pub(crate) fn new(
        mut values: BTreeMap<String, String>,
        kept: &BTreeSet<String>,
        permissions: &Permissions,
        cwd: PathBuf,
    ) -> Self {
        let mut removed = BTreeSet::new();
        if permissions.withholds_anything() {
            values.retain(|name, _| {
                let keep = !permissions.removes_variable(name)
                    || kept.contains(name) && !permissions.denies_variable(name);
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
pub(crate) fn op_bento_register_env_warning(
    scope: &mut v8::PinScope,
    state: &mut OpState,
    warn: v8::Local<v8::Function>,
) {
    state.put(RemovedVariableWarning(v8::Global::new(scope, warn)));
}

#[op2(reentrant)]
#[string]
pub(crate) fn bento_get_env(
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
pub(crate) fn bento_env(state: &OpState) -> BTreeMap<String, String> {
    state.borrow::<InstanceEnvironment>().values.lock().clone()
}

#[op2(fast)]
pub(crate) fn bento_set_env(
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
pub(crate) fn bento_delete_env(state: &mut OpState, #[string] key: &str) -> Result<(), JsErrorBox> {
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

#[op2]
#[string]
pub(crate) fn bento_cwd(state: &OpState) -> String {
    state.borrow::<InstanceEnvironment>().cwd.to_string_lossy().into_owned()
}
