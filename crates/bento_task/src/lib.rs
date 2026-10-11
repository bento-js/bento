//! Top-level `package.json` tasks: planning and trusted command execution.
//!
//! [`Plan::new`] validates the selected task graph before anything runs, and
//! [`TaskRunner::run`] executes the plan's commands in order. Task execution is a trusted
//! host operation; it is never available to guest code.

mod assignments;
mod cancellation;
mod definition;
mod plan;
mod process_group;
mod runner;

pub use cancellation::{Cancellation, Signal};
pub use definition::ShapeProblem;
pub use plan::{Plan, PlanError, Step};
pub use runner::{DEFAULT_GRACE_PERIOD, TaskError, TaskRunner};
