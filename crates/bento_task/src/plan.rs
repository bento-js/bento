use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use thiserror::Error;

use crate::definition::{ShapeProblem, TaskDefinition};
use bento_workspace::Manifest;

/// The commands one task invocation runs, in order, after the whole selected graph
/// has been validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The task that declared this command.
    pub task: String,
    pub command: String,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PlanError {
    #[error("no task named `{task}` in package.json")]
    UnknownTask { task: String },
    #[error("task `{task}` is invalid: {problem}")]
    InvalidTask { task: String, problem: ShapeProblem },
    #[error("task `{task}` depends on `{dependency}`, which is not a task")]
    MissingDependency { task: String, dependency: String },
    #[error("tasks depend on each other in a cycle: {}", .tasks.join(" → "))]
    Cycle { tasks: Vec<String> },
    #[error("task `{task}` has no command of its own to receive arguments")]
    ArgumentsWithoutCommand { task: String },
    #[error("task `{task}` runs several commands, so arguments have no single command to go to")]
    ArgumentsWithSeveralCommands { task: String },
}

impl Plan {
    /// Plans `task` from the manifest's top-level tasks. Dependencies come first,
    /// depth-first in declared order, each once; `arguments` are appended, shell-quoted,
    /// to the selected task's own single command.
    ///
    /// # Errors
    ///
    /// [`PlanError`]: the task or one of its dependencies is unknown or malformed, the
    /// dependencies form a cycle, or `arguments` were given for a task whose own commands
    /// cannot receive them.
    pub fn new(manifest: &Manifest, task: &str, arguments: &[String]) -> Result<Plan, PlanError> {
        if !manifest.tasks.contains_key(task) {
            return Err(PlanError::UnknownTask { task: task.to_owned() });
        }
        let mut planner = Planner {
            tasks: &manifest.tasks,
            planned: BTreeSet::new(),
            in_progress: Vec::new(),
            steps: Vec::new(),
        };
        planner.visit(task, arguments)?;
        Ok(Plan { steps: planner.steps })
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

struct Planner<'a> {
    tasks: &'a BTreeMap<String, Value>,
    planned: BTreeSet<String>,
    in_progress: Vec<String>,
    steps: Vec<Step>,
}

impl Planner<'_> {
    fn visit(&mut self, task: &str, arguments: &[String]) -> Result<(), PlanError> {
        if self.planned.contains(task) {
            return Ok(());
        }
        if let Some(start) = self.in_progress.iter().position(|visiting| visiting == task) {
            let mut tasks = self.in_progress[start..].to_vec();
            tasks.push(task.to_owned());
            return Err(PlanError::Cycle { tasks });
        }
        let TaskDefinition { commands, depends_on } = self.definition(task)?;
        let commands = with_arguments(task, commands, arguments)?;
        self.in_progress.push(task.to_owned());
        for dependency in &depends_on {
            if !self.tasks.contains_key(dependency) {
                let task = task.to_owned();
                return Err(PlanError::MissingDependency { task, dependency: dependency.clone() });
            }
            self.visit(dependency, &[])?;
        }
        self.in_progress.pop();
        self.planned.insert(task.to_owned());
        let task = task.to_owned();
        self.steps.extend(commands.into_iter().map(|command| Step { task: task.clone(), command }));
        Ok(())
    }

    fn definition(&self, task: &str) -> Result<TaskDefinition, PlanError> {
        let declaration = &self.tasks[task];
        TaskDefinition::parse(declaration)
            .map_err(|problem| PlanError::InvalidTask { task: task.to_owned(), problem })
    }
}

fn with_arguments(
    task: &str,
    commands: Vec<String>,
    arguments: &[String],
) -> Result<Vec<String>, PlanError> {
    if arguments.is_empty() {
        return Ok(commands);
    }
    match commands.as_slice() {
        [] => Err(PlanError::ArgumentsWithoutCommand { task: task.to_owned() }),
        [command] => {
            let quoted: Vec<Cow<str>> =
                arguments.iter().map(|argument| shell_quote(argument)).collect();
            Ok(vec![format!("{command} {}", quoted.join(" "))])
        }
        _ => Err(PlanError::ArgumentsWithSeveralCommands { task: task.to_owned() }),
    }
}

/// Quotes `argument` for POSIX `sh` so it arrives as exactly one literal word.
fn shell_quote(argument: &str) -> Cow<'_, str> {
    let is_plain = |byte: u8| byte.is_ascii_alphanumeric() || b"-_./=:@%+,".contains(&byte);
    if !argument.is_empty() && argument.bytes().all(is_plain) {
        Cow::Borrowed(argument)
    } else {
        Cow::Owned(format!("'{}'", argument.replace('\'', r"'\''")))
    }
}
