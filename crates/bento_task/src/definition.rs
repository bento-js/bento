use serde_json::{Map, Value};
use thiserror::Error;

/// One declared task in its validated form.
pub(crate) struct TaskDefinition {
    pub commands: Vec<String>,
    pub depends_on: Vec<String>,
}

/// Why a task declaration is not a valid task.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ShapeProblem {
    #[error(
        "expected a command string, an array of command strings, or an object with `command` and/or `dependsOn`"
    )]
    UnknownShape,
    #[error("a command must not be empty")]
    EmptyCommand,
    #[error("a command array must not be empty")]
    EmptyArray,
    #[error("every command in an array must be a string")]
    NonStringCommand,
    #[error("`command` must be a single command string")]
    InvalidCommandField,
    #[error("`dependsOn` must be an array of task names")]
    InvalidDependsOn,
    #[error("a task object needs a `command`, at least one `dependsOn` entry, or both")]
    NothingToRun,
    #[error("unknown field `{0}`; a task object has only `command` and `dependsOn`")]
    UnknownField(String),
}

impl TaskDefinition {
    pub(crate) fn parse(declaration: &Value) -> Result<TaskDefinition, ShapeProblem> {
        match declaration {
            Value::String(text) => {
                Ok(TaskDefinition { commands: vec![command(text)?], depends_on: Vec::new() })
            }
            Value::Array(items) => command_array(items),
            Value::Object(fields) => object(fields),
            _ => Err(ShapeProblem::UnknownShape),
        }
    }
}

fn command(text: &str) -> Result<String, ShapeProblem> {
    if text.trim().is_empty() { Err(ShapeProblem::EmptyCommand) } else { Ok(text.to_owned()) }
}

fn command_array(items: &[Value]) -> Result<TaskDefinition, ShapeProblem> {
    if items.is_empty() {
        return Err(ShapeProblem::EmptyArray);
    }
    let commands = items
        .iter()
        .map(|item| item.as_str().ok_or(ShapeProblem::NonStringCommand).and_then(command))
        .collect::<Result<_, _>>()?;
    Ok(TaskDefinition { commands, depends_on: Vec::new() })
}

fn object(fields: &Map<String, Value>) -> Result<TaskDefinition, ShapeProblem> {
    if let Some(unknown) =
        fields.keys().find(|key| !matches!(key.as_str(), "command" | "dependsOn"))
    {
        return Err(ShapeProblem::UnknownField(unknown.clone()));
    }
    let commands = match fields.get("command") {
        None => Vec::new(),
        Some(Value::String(text)) => vec![command(text)?],
        Some(_) => return Err(ShapeProblem::InvalidCommandField),
    };
    let depends_on = match fields.get("dependsOn") {
        None => Vec::new(),
        Some(Value::Array(names)) => names.iter().map(dependency_name).collect::<Result<_, _>>()?,
        Some(_) => return Err(ShapeProblem::InvalidDependsOn),
    };
    if commands.is_empty() && depends_on.is_empty() {
        return Err(ShapeProblem::NothingToRun);
    }
    Ok(TaskDefinition { commands, depends_on })
}

fn dependency_name(value: &Value) -> Result<String, ShapeProblem> {
    match value.as_str() {
        Some(name) if !name.is_empty() => Ok(name.to_owned()),
        _ => Err(ShapeProblem::InvalidDependsOn),
    }
}
