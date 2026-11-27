use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use deno_core::{Extension, FastStaticString, OpDecl, OpState, op2, serde_v8, v8};
use deno_error::JsErrorBox;

use bento_host::CEILING_ENV;

use crate::environment::InstanceEnvironment;
use crate::permissions::Authority;

/// Deno's spawn operations, kept under other names so Bento's replacements can call them.
struct OriginalSpawnOps {
    child: v8::Global<v8::Function>,
    node_child: v8::Global<v8::Function>,
    sync: v8::Global<v8::Function>,
}

pub(crate) fn extension() -> Extension {
    let mut extension = bento_subprocess_ops::init();
    let upstream = deno_runtime::deno_process::deno_process::init(None);
    let original = |name: &str, renamed: &'static str, renamed_fast: FastStaticString| {
        let mut op =
            *upstream.ops.iter().find(|op| op.name == name).expect("upstream spawn op exists");
        op.name = renamed;
        op.name_fast = renamed_fast;
        op
    };
    extension.ops.to_mut().extend([
        original(
            "op_spawn_child",
            "op_bento_original_spawn_child",
            deno_core::ascii_str!("op_bento_original_spawn_child"),
        ),
        original(
            "op_node_spawn_child",
            "op_bento_original_node_spawn_child",
            deno_core::ascii_str!("op_bento_original_node_spawn_child"),
        ),
        original(
            "op_spawn_sync",
            "op_bento_original_spawn_sync",
            deno_core::ascii_str!("op_bento_original_spawn_sync"),
        ),
    ]);
    extension
}

pub(crate) fn replacement(name: &str) -> Option<OpDecl> {
    Some(match name {
        "op_spawn_child" => bento_spawn_child(),
        "op_node_spawn_child" => bento_node_spawn_child(),
        "op_spawn_sync" => bento_spawn_sync(),
        _ => return None,
    })
}

#[op2(nofast)]
fn op_bento_register_spawn_ops(
    scope: &mut v8::PinScope,
    state: &mut OpState,
    child: v8::Local<v8::Function>,
    node_child: v8::Local<v8::Function>,
    sync: v8::Local<v8::Function>,
) {
    state.put(OriginalSpawnOps {
        child: v8::Global::new(scope, child),
        node_child: v8::Global::new(scope, node_child),
        sync: v8::Global::new(scope, sync),
    });
}

#[op2(reentrant, stack_trace)]
fn bento_spawn_child<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: Rc<RefCell<OpState>>,
    arguments: v8::Local<'s, v8::Object>,
    api_name: v8::Local<'s, v8::Value>,
) -> Result<Option<v8::Local<'s, v8::Value>>, JsErrorBox> {
    let original = v8::Local::new(scope, &state.borrow().borrow::<OriginalSpawnOps>().child);
    spawn(scope, &state, original, arguments, Some(api_name))
}

#[op2(reentrant, stack_trace)]
fn bento_node_spawn_child<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: Rc<RefCell<OpState>>,
    arguments: v8::Local<'s, v8::Object>,
    api_name: v8::Local<'s, v8::Value>,
) -> Result<Option<v8::Local<'s, v8::Value>>, JsErrorBox> {
    let original = v8::Local::new(scope, &state.borrow().borrow::<OriginalSpawnOps>().node_child);
    spawn(scope, &state, original, arguments, Some(api_name))
}

#[op2(reentrant, stack_trace)]
fn bento_spawn_sync<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: Rc<RefCell<OpState>>,
    arguments: v8::Local<'s, v8::Object>,
) -> Result<Option<v8::Local<'s, v8::Value>>, JsErrorBox> {
    let original = v8::Local::new(scope, &state.borrow().borrow::<OriginalSpawnOps>().sync);
    spawn(scope, &state, original, arguments, None)
}

/// Starts the process from the instance rather than the host process: in the guest's working
/// directory unless the call names another, with the instance's environment where the call
/// would inherit one, and, from an instance that withholds anything, its current permissions
/// as the ceiling of any `bento` the process starts.
fn spawn<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: &Rc<RefCell<OpState>>,
    original: v8::Local<'s, v8::Function>,
    arguments: v8::Local<'s, v8::Object>,
    api_name: Option<v8::Local<'s, v8::Value>>,
) -> Result<Option<v8::Local<'s, v8::Value>>, JsErrorBox> {
    let (environment, authority) = {
        let state = state.borrow();
        (state.borrow::<InstanceEnvironment>().clone(), state.borrow::<Arc<Authority>>().clone())
    };
    if property(scope, arguments, "cwd").is_none_or(|cwd| cwd.is_null_or_undefined()) {
        let cwd = environment.cwd.to_string_lossy();
        let cwd = v8::String::new(scope, &cwd).ok_or_else(|| string_error("cwd"))?;
        set(scope, arguments, "cwd", cwd.into())?;
    }
    let clear = property(scope, arguments, "clearEnv").is_some_and(|clear| clear.is_true());
    let given = property(scope, arguments, "env").ok_or_else(|| string_error("env"))?;
    let given: Vec<(String, String)> = serde_v8::from_v8(scope, given)
        .map_err(|error| JsErrorBox::type_error(error.to_string()))?;
    let mut env: BTreeMap<String, String> =
        if clear { BTreeMap::new() } else { environment.values.lock().clone() };
    env.extend(given);
    let ceiling = authority.ceiling();
    if ceiling.withholds_anything() {
        let ceiling = ceiling
            .to_ceiling_value()
            .map_err(|error| JsErrorBox::type_error(error.to_string()))?;
        env.insert(CEILING_ENV.to_owned(), ceiling);
    }
    let env: Vec<(String, String)> = env.into_iter().collect();
    let env =
        serde_v8::to_v8(scope, env).map_err(|error| JsErrorBox::type_error(error.to_string()))?;
    set(scope, arguments, "env", env)?;
    let clear = v8::Boolean::new(scope, true).into();
    set(scope, arguments, "clearEnv", clear)?;
    let receiver = v8::undefined(scope).into();
    let call_arguments: Vec<v8::Local<v8::Value>> =
        std::iter::once(arguments.into()).chain(api_name).collect();
    Ok(original.call(scope, receiver, &call_arguments))
}

fn property<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    name: &str,
) -> Option<v8::Local<'s, v8::Value>> {
    let key = v8::String::new(scope, name)?;
    object.get(scope, key.into())
}

fn set<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    name: &str,
    value: v8::Local<'s, v8::Value>,
) -> Result<(), JsErrorBox> {
    let key = v8::String::new(scope, name).ok_or_else(|| string_error(name))?;
    object.set(scope, key.into(), value).ok_or_else(|| string_error(name))?;
    Ok(())
}

fn string_error(name: &str) -> JsErrorBox {
    JsErrorBox::type_error(format!("cannot prepare the child process {name}"))
}

deno_core::extension!(
    bento_subprocess_ops,
    ops = [op_bento_register_spawn_ops],
    esm_entry_point = "ext:bento_subprocess_ops/subprocess_ops.mjs",
    // Embedded rather than `esm = [dir ...]`, which reads the file from the build machine at
    // run time and leaves a moved executable unable to start.
    customizer = |extension: &mut deno_core::Extension| {
        const SOURCES: &[deno_core::ExtensionFileSource] = &[deno_core::ExtensionFileSource::new(
            "ext:bento_subprocess_ops/subprocess_ops.mjs",
            deno_core::ascii_str_include!("subprocess_ops.mjs"),
        )];
        extension.esm_files = std::borrow::Cow::Borrowed(SOURCES);
    },
);
