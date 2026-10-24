//! Embeddable runtime for Node-compatible JavaScript applications.
//!
//! One [`RunningRuntime`] runs one guest program, with any workers it starts, on its own
//! thread, launched by [`start`] from a [`RuntimeConfig`] that fixes the instance's grants,
//! environment, arguments, and host modules for its lifetime. Dependencies must already be
//! installed: execution downloads nothing and starts no Node, Deno, Bun, or Bento subprocess.
//! The runtime prints none of its own diagnostics; the guest's console output goes to the
//! process's stdout and stderr as it would in Node.
//!
//! ```no_run
//! # use std::collections::{BTreeMap, BTreeSet};
//! # use std::path::PathBuf;
//! # use std::time::Duration;
//! #
//! # use bento_host::{Grants, Scope};
//! # use bento_runtime::{HostError, HostModule, RunOutcome, RuntimeConfig, start};
//! # use serde_json::json;
//! #
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let root = std::fs::canonicalize("./application")?;
//! let module = HostModule::new("host:demo").function("greet", |arguments| async move {
//!     let name = arguments[0].as_str().ok_or_else(|| HostError {
//!         name: "TypeError".into(),
//!         message: "greet requires a string".into(),
//!     })?;
//!     Ok(json!({ "message": format!("Hello, {name}") }))
//! });
//! let config = RuntimeConfig {
//!     project_root: root.clone(),
//!     cwd: root.clone(),
//!     grants: Grants {
//!         read: Scope::Only(BTreeSet::from([root.clone()])),
//!         env: Scope::Only(BTreeSet::from(["MODE".into()])),
//!         ..Grants::default()
//!     },
//!     env: BTreeMap::from([("MODE".into(), "embedded".into())]),
//!     argv: vec![],
//!     host_modules: vec![module],
//!     shutdown_deadline: Duration::from_secs(5),
//! };
//! let runtime = start(config, &PathBuf::from("main.ts"))?;
//! for diagnostic in runtime.diagnostics() {
//!     eprintln!("{diagnostic}");
//! }
//! match runtime.wait() {
//!     RunOutcome::Completed => {}
//!     RunOutcome::Exited(code) => println!("Guest exited: {code}"),
//!     RunOutcome::Failed(error) => eprintln!("{:?}: {}", error.kind, error.message),
//!     RunOutcome::Shutdown(report) => println!("Shutdown: {:?}", report.outcome),
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod assembly;
mod cjs;
mod error;
mod feature_ops;
mod features;
mod fs_ops;
mod host;
mod host_ops;
mod import_position;
mod lifecycle;
mod loader;
mod node;
mod ops;
mod package_lookup;
mod permissions;
mod signals;
mod sources;
mod watch_inputs;
mod workers;

use std::any::Any;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::thread::JoinHandle;
use std::time::Duration;

use bento_host::Grants;

pub use bento_compiler::{Diagnostic, Severity};
pub use error::{ErrorKind, RunOutcome, RuntimeError, SourceLocation, StartError};
pub use host::{HostError, HostModule};
pub use lifecycle::{RuntimeHandle, ShutdownOutcome, ShutdownReport, Signal};
pub use package_lookup::package_lookup_paths;

/// Everything one instance is given before it starts; nothing here can change afterwards.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// Directory whose `package.json` and `tsconfig.json` the instance runs under, and from
    /// which installed dependencies are resolved. [`start`] canonicalizes it.
    pub project_root: PathBuf,
    /// The guest's `process.cwd()`, and the directory a relative entry path is resolved
    /// against. [`start`] canonicalizes it.
    pub cwd: PathBuf,
    /// Files, endpoints, environment variables, and addons the guest may use. Anything not
    /// granted fails inside the guest rather than prompting.
    pub grants: Grants,
    /// Starting `process.env` for the guest. Only variables covered by `grants.env` are
    /// visible to it, and its writes stay inside the instance.
    pub env: BTreeMap<String, String>,
    /// Guest `process.argv` beyond the executable and the entry path.
    pub argv: Vec<String>,
    /// Modules the guest may import under their `host:` specifiers.
    pub host_modules: Vec<HostModule>,
    /// How long a requested shutdown may run before the isolate is terminated; expiry
    /// reports [`ShutdownOutcome::TimedOut`].
    pub shutdown_deadline: Duration,
}

/// A started instance, owning the thread the guest runs on. Dropping one shuts it down and
/// discards its outcome; [`RunningRuntime::wait`] is the only way to read it.
pub struct RunningRuntime {
    thread: Option<JoinHandle<RunOutcome>>,
    handle: RuntimeHandle,
    diagnostics: Vec<Diagnostic>,
}

impl RuntimeConfig {
    /// Denies every grant, leaves environment, arguments, and host modules empty, and runs
    /// the guest in the project root with a five-second shutdown deadline.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        Self {
            cwd: project_root.clone(),
            project_root,
            grants: Grants::default(),
            env: BTreeMap::new(),
            argv: Vec::new(),
            host_modules: Vec::new(),
            shutdown_deadline: Duration::from_secs(5),
        }
    }
}

static V8_PLATFORM: LazyLock<()> = LazyLock::new(|| deno_core::JsRuntime::init_platform(None));

/// Launches `entry` on a new runtime thread and returns while the guest is still running.
///
/// `entry` is taken relative to `config.cwd` unless it is absolute. Project root, working
/// directory, and entry are canonicalized, so all three must exist.
///
/// # Errors
///
/// [`ErrorKind::Startup`] when one of those three paths cannot be resolved, when a host
/// module specifier is not a unique `host:` name, or when the runtime or shutdown-monitor
/// thread cannot be spawned; [`ErrorKind::Transform`] when the project's `tsconfig.json`
/// cannot be read or understood.
pub fn start(mut config: RuntimeConfig, entry: &Path) -> Result<RunningRuntime, StartError> {
    config.project_root = std::fs::canonicalize(&config.project_root)
        .map_err(|error| RuntimeError::startup(format!("project root: {error}")))?;
    config.cwd = std::fs::canonicalize(&config.cwd)
        .map_err(|error| RuntimeError::startup(format!("working directory: {error}")))?;
    let entry = if entry.is_absolute() { entry.to_owned() } else { config.cwd.join(entry) };
    let entry = std::fs::canonicalize(&entry)
        .map_err(|error| RuntimeError::startup(format!("{}: {error}", entry.display())))?;
    let modules = host::HostModules::new(std::mem::take(&mut config.host_modules))?;
    let (sources, diagnostics) = sources::Sources::load(&config.project_root)?;
    let sources = Arc::new(sources);
    let (control, commands) = lifecycle::Control::new(config.shutdown_deadline)?;
    let handle = RuntimeHandle { control: control.clone() };
    LazyLock::force(&V8_PLATFORM);
    let thread = std::thread::Builder::new()
        .name("bento-runtime".into())
        .spawn(move || run_thread(config, entry, modules, sources, control, commands))
        .map_err(|error| {
            let error = RuntimeError::startup(error.to_string());
            handle.control.complete(&RunOutcome::Failed(error.clone()));
            error
        })?;
    Ok(RunningRuntime { thread: Some(thread), handle, diagnostics })
}

fn run_thread(
    config: RuntimeConfig,
    entry: PathBuf,
    modules: host::HostModules,
    sources: Arc<sources::Sources>,
    control: Arc<lifecycle::Control>,
    commands: tokio::sync::mpsc::UnboundedReceiver<lifecycle::Command>,
) -> RunOutcome {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let outcome = RunOutcome::Failed(RuntimeError::startup(error.to_string()));
            control.complete(&outcome);
            return outcome;
        }
    };
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, assembly::run(config, entry, modules, sources, control, commands))
}

impl RunningRuntime {
    /// A handle for signalling or shutting this instance down, usable from another thread.
    pub fn handle(&self) -> RuntimeHandle {
        self.handle.clone()
    }

    /// The project's `tsconfig.json` options that Bento handles differently from `tsc`, found
    /// once while this instance started. The runtime prints nothing; the host decides how to
    /// report them.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the guest has already ended, so [`RunningRuntime::wait`] would return without
    /// blocking.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Blocks until the guest has ended and the instance has been disposed, then reports how
    /// it ended. A panic on the runtime thread is reported as [`RunOutcome::Failed`] instead
    /// of being resumed here.
    ///
    /// # Panics
    ///
    /// Never: the thread is taken only here and in `Drop`, and this consumes the instance.
    pub fn wait(mut self) -> RunOutcome {
        match self.thread.take().expect("runtime thread available").join() {
            Ok(outcome) => outcome,
            Err(payload) => {
                RunOutcome::Failed(RuntimeError::startup(panic_message(payload.as_ref())))
            }
        }
    }
}

impl Drop for RunningRuntime {
    /// Requests shutdown and blocks until the instance is disposed, unless the shutdown
    /// deadline expired first. The outcome is discarded, a runtime-thread panic included;
    /// [`RunningRuntime::wait`] is how a host keeps it.
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let report = self.handle.shutdown();
            if report.outcome != ShutdownOutcome::TimedOut {
                let _ = thread.join();
            }
        }
    }
}

/// The text a panic carried, which is a payload only for `panic!` with a message.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
    match text {
        Some(text) => format!("runtime thread panicked: {text}"),
        None => "runtime thread panicked".to_owned(),
    }
}
