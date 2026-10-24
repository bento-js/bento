use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::StartError;

type HostFuture = Pin<Box<dyn Future<Output = Result<Value, HostError>> + Send>>;
type HostFunction = Arc<dyn Fn(Value) -> HostFuture + Send + Sync>;

/// A host function's failure, rethrown in the guest as an `Error` carrying these two fields.
#[derive(Clone, Debug, Serialize)]
pub struct HostError {
    /// The thrown error's `name`, which is what guest code branches on.
    pub name: String,
    /// The thrown error's `message`.
    pub message: String,
}

/// Rust functions the guest imports as the named exports of one `host:` module.
#[derive(Clone)]
pub struct HostModule {
    specifier: String,
    functions: BTreeMap<String, HostFunction>,
}

impl HostModule {
    /// `specifier` must be `host:` followed by a module name, unique among the modules of
    /// one config; [`start`](crate::start) rejects the rest.
    pub fn new(specifier: impl Into<String>) -> Self {
        Self { specifier: specifier.into(), functions: BTreeMap::new() }
    }

    /// Exports `handler` under `name`, replacing any earlier function of that name.
    ///
    /// The handler receives the call's arguments as a JSON array and returns JSON the guest
    /// sees as a plain value. It is polled on the runtime thread, so blocking inside it
    /// stalls the guest's event loop.
    pub fn function<F, Fut>(mut self, name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, HostError>> + Send + 'static,
    {
        self.functions.insert(name.into(), Arc::new(move |arguments| Box::pin(handler(arguments))));
        self
    }
}

impl fmt::Debug for HostModule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostModule")
            .field("specifier", &self.specifier)
            .field("functions", &self.functions.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Clone, Default)]
pub(crate) struct HostModules(BTreeMap<String, HostModule>);

impl HostModules {
    pub(crate) fn new(modules: Vec<HostModule>) -> Result<Self, StartError> {
        let mut registered = BTreeMap::new();
        for module in modules {
            if !module.specifier.starts_with("host:") || module.specifier.len() == 5 {
                return Err(StartError::startup(
                    "host module specifiers must start with host: and name a module",
                ));
            }
            let specifier = module.specifier.clone();
            if registered.insert(specifier.clone(), module).is_some() {
                return Err(StartError::startup(format!(
                    "duplicate host module registration: {specifier}"
                )));
            }
        }
        Ok(Self(registered))
    }

    pub(crate) fn contains(&self, specifier: &str) -> bool {
        self.0.contains_key(specifier)
    }

    pub(crate) fn handler(&self, specifier: &str, name: &str) -> Option<HostFunction> {
        self.0.get(specifier)?.functions.get(name).cloned()
    }

    pub(crate) fn source(&self, specifier: &str) -> Option<String> {
        let module = self.0.get(specifier)?;
        let specifier = serde_json::to_string(specifier).expect("string serialization");
        let mut source = "const invoke = globalThis[Symbol.for('bento.host.invoke')];\n".to_owned();
        for (index, name) in module.functions.keys().enumerate() {
            let name = serde_json::to_string(name).expect("string serialization");
            source.push_str(&format!("const handler{index} = (...args) => invoke({specifier}, {name}, args); export {{ handler{index} as {name} }};\n"));
        }
        Some(source)
    }
}
