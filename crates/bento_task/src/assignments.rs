//! The variables a task command assigns for a nested `bento` launch. The launch must treat
//! them as explicit even when they repeat a value loaded from a file, and the environment
//! it inherits is identical either way, so only the command text can tell. Where the text
//! alone cannot decide, a name counts: that keeps the user's value.

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::env_arguments;
use crate::shell_words::{simple_commands, unquoted};

/// Words that may precede a command without being its name.
const RESERVED_WORDS: [&str; 9] = ["!", "{", "if", "then", "elif", "else", "while", "until", "do"];

/// Regular builtins that start no program, so a prefix assignment on them reaches nothing.
/// Special builtins such as `:` and `exec` are absent: a prefix assignment on them persists
/// in the shell.
const NON_LAUNCHING_BUILTINS: [&str; 21] = [
    "[", "alias", "bg", "cd", "echo", "false", "fg", "getopts", "hash", "jobs", "kill", "printf",
    "pwd", "read", "test", "true", "type", "ulimit", "umask", "unalias", "wait",
];

/// Builtins whose `NAME=value` arguments assign variables for the rest of the shell.
const EXPORTING_BUILTINS: [&str; 2] = ["export", "readonly"];

/// Utilities that run their first operand as a command, with the same environment.
const WRAPPERS: [&str; 3] = ["command", "exec", "nohup"];

/// Names assigned in `command`'s text that a nested `bento` launch it starts may see.
pub(crate) fn assigned_names(command: &str) -> BTreeSet<String> {
    simple_commands(command).iter().flat_map(|words| names_reaching_a_launch(words)).collect()
}

/// What a simple command does with the variables assigned around it.
enum Effect {
    /// A command such as `true` or `echo` that starts no program; prefix assignments on it
    /// reach nothing.
    NoProgram,
    /// Names assigned for the program the command runs or for the rest of the shell.
    Assigns(Vec<String>),
}

/// The names a nested launch started by this simple command, or by a later command in the
/// same shell, sees assigned by the command text.
fn names_reaching_a_launch(words: &[String]) -> Vec<String> {
    let reserved = words.iter().take_while(|word| RESERVED_WORDS.contains(&word.as_str())).count();
    let words = &words[reserved..];
    let prefixes = words.iter().take_while(|word| assignment_name(word).is_some()).count();
    let (prefix_words, command) = words.split_at(prefixes);
    match command_effect(command) {
        Effect::NoProgram => Vec::new(),
        Effect::Assigns(names) => {
            assignment_names(prefix_words.iter().map(String::as_str)).chain(names).collect()
        }
    }
}

/// `command` follows the prefix assignments; empty for a bare assignment statement, which
/// assigns for the rest of the shell.
fn command_effect(command: &[String]) -> Effect {
    let Some((name, arguments)) = command.split_first() else {
        return Effect::Assigns(Vec::new());
    };
    let name = unquoted(name);
    let arguments: Vec<Cow<'_, str>> = arguments.iter().map(|word| unquoted(word)).collect();
    if EXPORTING_BUILTINS.contains(&name.as_ref()) {
        let operands = arguments.iter().filter(|word| !word.starts_with('-'));
        return Effect::Assigns(assignment_names(operands.map(Cow::as_ref)).collect());
    }
    program_effect(&name, &arguments)
}

/// The effect of running the program `name` with `arguments`, all already unquoted. Any
/// program other than a known builtin may be, or may start, a nested `bento`.
fn program_effect(name: &str, arguments: &[Cow<'_, str>]) -> Effect {
    if NON_LAUNCHING_BUILTINS.contains(&name) {
        return Effect::NoProgram;
    }
    if WRAPPERS.contains(&name) {
        return operands_effect(skip_options(arguments), Vec::new());
    }
    if name == "env" {
        return env_effect(arguments);
    }
    Effect::Assigns(Vec::new())
}

/// `env`'s `NAME=value` operands apply to the utility it runs. Without a utility it prints
/// the environment, and they reach nothing.
fn env_effect(arguments: &[Cow<'_, str>]) -> Effect {
    let operands = env_arguments::operands(arguments);
    let assignments = operands.iter().take_while(|word| assignment_name(word).is_some()).count();
    let (assignment_words, utility) = operands.split_at(assignments);
    if utility.is_empty() {
        return Effect::NoProgram;
    }
    let names = assignment_names(assignment_words.iter().map(Cow::as_ref)).collect();
    operands_effect(utility, names)
}

/// The effect of `names`, assigned for the program that `command` names. Without one they
/// are kept: `exec` alone is a special builtin, whose prefix assignments persist.
fn operands_effect(command: &[Cow<'_, str>], names: Vec<String>) -> Effect {
    let Some((name, arguments)) = command.split_first() else {
        return Effect::Assigns(names);
    };
    match program_effect(name, arguments) {
        Effect::NoProgram => Effect::NoProgram,
        Effect::Assigns(inner) => Effect::Assigns(names.into_iter().chain(inner).collect()),
    }
}

fn skip_options<'a, 'b>(arguments: &'a [Cow<'b, str>]) -> &'a [Cow<'b, str>] {
    let options = arguments.iter().take_while(|word| word.starts_with('-')).count();
    &arguments[options..]
}

fn assignment_names<'a>(words: impl Iterator<Item = &'a str>) -> impl Iterator<Item = String> {
    words.filter_map(assignment_name).map(str::to_owned)
}

/// The name a word assigns, when it starts with a name and `=`. Quotes and escapes are not
/// name characters, so a raw word with a quoted name never matches.
fn assignment_name(word: &str) -> Option<&str> {
    let (name, _) = word.split_once('=')?;
    let mut characters = name.chars();
    let starts_a_name =
        characters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    let is_name = starts_a_name
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
    is_name.then_some(name)
}
