use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{OpState, op2};
use deno_error::JsErrorBox;
use serde::Serialize;
use serde_json::Value;

use crate::host::{HostError, HostModules};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostResponse {
    value: Option<Value>,
    error: Option<HostError>,
}

#[op2]
#[string]
pub(crate) async fn op_bento_host_call(
    state: Rc<RefCell<OpState>>,
    #[string] specifier: String,
    #[string] name: String,
    #[serde] arguments: serde_json::Value,
) -> Result<String, JsErrorBox> {
    let handler =
        state.borrow().borrow::<HostModules>().handler(&specifier, &name).ok_or_else(|| {
            JsErrorBox::type_error(format!("Unknown host function {specifier}:{name}"))
        })?;
    let response = match handler(arguments).await {
        Ok(value) => HostResponse { value: Some(value), error: None },
        Err(error) => HostResponse { value: None, error: Some(error) },
    };
    // serde_v8 creates null-prototype maps; JSON preserves ordinary guest object prototypes.
    serde_json::to_string(&response).map_err(JsErrorBox::from_err)
}
