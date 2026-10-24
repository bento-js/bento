use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;

use bento_runtime::RuntimeConfig;

use crate::ending::{self, Stop};
use crate::error::CliError;
use crate::events::{Event, next_event, send_signals};
use crate::instance::Instance;
use crate::launch::Launch;

/// Runs the script at `entry` in a runtime instance until it ends, and returns the exit code
/// it ended with. The first SIGINT or SIGTERM goes to the script; a second ends `bento` at once.
pub(crate) fn run(
    launch: Launch,
    entry: &Path,
    invocation_directory: &Path,
    arguments: &[String],
) -> Result<ExitCode, CliError> {
    let config = runtime_config(launch, invocation_directory, arguments);
    let deadline = config.shutdown_deadline;
    let (events, received) = mpsc::channel();
    send_signals(&events)?;
    let instance = Instance::start(config, entry, &events).map_err(CliError::Script)?;
    let mut stop: Option<Stop> = None;
    loop {
        match next_event(&received, stop.as_ref().map(Stop::deadline)) {
            Some(Event::Ended(outcome)) => {
                return match &stop {
                    Some(stop) => stop.exit_code(outcome),
                    None => ending::finished(outcome),
                };
            }
            Some(Event::Signal(signal)) if stop.is_none() => {
                stop = Some(Stop::begin(signal, &instance, deadline));
            }
            Some(Event::Signal(signal)) => ending::force(signal),
            Some(Event::Changed(_)) => unreachable!("only `--watch` watches for changes"),
            None => {}
        }
        if let Some(stop) = &stop {
            stop.enforce_deadline();
        }
    }
}

/// The runtime configuration for running the script `launch` prepared: its grants and
/// environment, the directory `bento` runs in, and the words after the script.
pub(crate) fn runtime_config(
    launch: Launch,
    invocation_directory: &Path,
    arguments: &[String],
) -> RuntimeConfig {
    RuntimeConfig {
        cwd: invocation_directory.to_owned(),
        grants: launch.grants,
        env: launch.environment.vars().clone(),
        argv: arguments.to_vec(),
        ..RuntimeConfig::new(launch.project.root())
    }
}
