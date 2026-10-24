//! Embeddable runtime for Node-compatible JavaScript applications.

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

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub project_root: PathBuf,
    pub cwd: PathBuf,
    pub grants: Grants,
    pub env: BTreeMap<String, String>,
    pub argv: Vec<String>,
    pub host_modules: Vec<HostModule>,
    pub shutdown_deadline: Duration,
}

pub struct RunningRuntime {
    thread: Option<JoinHandle<RunOutcome>>,
    handle: RuntimeHandle,
    diagnostics: Vec<Diagnostic>,
}

impl RuntimeConfig {
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
    pub fn handle(&self) -> RuntimeHandle {
        self.handle.clone()
    }

    /// The project's `tsconfig.json` options that Bento handles differently from `tsc`, found
    /// once while this instance started. The runtime prints nothing; the host decides how to
    /// report them.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn wait(mut self) -> RunOutcome {
        match self.thread.take().expect("runtime thread available").join() {
            Ok(outcome) => outcome,
            Err(_) => RunOutcome::Failed(RuntimeError::startup("runtime thread panicked")),
        }
    }
}

impl Drop for RunningRuntime {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let report = self.handle.shutdown();
            if report.outcome != ShutdownOutcome::TimedOut {
                let _ = thread.join();
            }
        }
    }
}
