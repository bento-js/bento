//! Embeddable runtime for Node-compatible JavaScript applications.

mod assembly;
mod cjs;
mod error;
mod loader;
mod node;
mod ops;
mod permissions;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::thread::JoinHandle;

use bento_host::Grants;

pub use error::{ErrorKind, RunOutcome, RuntimeError, StartError};

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub project_root: PathBuf,
    pub cwd: PathBuf,
    pub grants: Grants,
    pub env: BTreeMap<String, String>,
    pub argv: Vec<String>,
}

pub struct RunningRuntime {
    thread: JoinHandle<RunOutcome>,
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
    LazyLock::force(&V8_PLATFORM);
    let thread = std::thread::Builder::new()
        .name("bento-runtime".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(error) => return RunOutcome::Failed(RuntimeError::startup(error.to_string())),
            };
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, assembly::run(config, entry))
        })
        .map_err(|error| RuntimeError::startup(error.to_string()))?;
    Ok(RunningRuntime { thread })
}

impl RunningRuntime {
    pub fn wait(self) -> RunOutcome {
        match self.thread.join() {
            Ok(outcome) => outcome,
            Err(_) => RunOutcome::Failed(RuntimeError::startup("runtime thread panicked")),
        }
    }
}
