use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use deno_core::error::ResourceError;
use deno_core::{AsyncRefCell, OpState, RcRef, Resource, ResourceId, op2};
use deno_error::JsErrorBox;
use tokio::sync::watch;

use crate::lifecycle::Control;

struct VirtualSignal {
    id: Cell<Option<u64>>,
    control: Arc<Control>,
    receiver: AsyncRefCell<watch::Receiver<()>>,
}

impl Resource for VirtualSignal {
    fn name(&self) -> Cow<'_, str> {
        "bentoSignal".into()
    }

    fn close(self: Rc<Self>) {
        if let Some(id) = self.id.take() {
            self.control.unregister_signal(id);
        }
    }
}

impl Drop for VirtualSignal {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            self.control.unregister_signal(id);
        }
    }
}

#[op2(fast)]
#[smi]
pub(crate) fn bento_signal_bind(
    state: &mut OpState,
    #[string] name: &str,
) -> Result<ResourceId, JsErrorBox> {
    let number = deno_signals::signal_str_to_int(name)
        .map_err(|error| JsErrorBox::type_error(error.to_string()))?;
    if deno_signals::is_forbidden(number) {
        return Err(JsErrorBox::type_error(format!("Binding to signal '{name}' is not allowed")));
    }
    let control = state.borrow::<Arc<Control>>().clone();
    let (sender, receiver) = watch::channel(());
    let id = control.register_signal(name.into(), sender);
    Ok(state.resource_table.add(VirtualSignal {
        id: Cell::new(Some(id)),
        control,
        receiver: AsyncRefCell::new(receiver),
    }))
}

#[op2]
pub(crate) async fn bento_signal_poll(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
) -> Result<bool, ResourceError> {
    let resource = state.borrow().resource_table.get::<VirtualSignal>(rid)?;
    let mut receiver = RcRef::map(&resource, |signal| &signal.receiver).borrow_mut().await;
    Ok(receiver.changed().await.is_err())
}

#[op2(fast)]
pub(crate) fn bento_signal_unbind(
    state: &mut OpState,
    #[smi] rid: ResourceId,
) -> Result<(), ResourceError> {
    let resource = state.resource_table.take::<VirtualSignal>(rid)?;
    resource.close();
    Ok(())
}
