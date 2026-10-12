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

/// Builtins that run their first operand as a command in the shell itself, so a builtin it
/// names, such as `export`, has the effect it has as the command word.
const SHELL_WRAPPERS: [&str; 2] = ["builtin", "command"];

/// Utilities that execute their first operand as a program found on `PATH`, with the same
/// environment. That program is never one of the shell's builtins: `nohup export` runs a
/// program named `export`, and assigns nothing in the shell.
const PROGRAM_WRAPPERS: [&str; 2] = ["exec", "nohup"];

/// The modeled utilities that are also programs, so a path to one runs that utility:
/// `/usr/bin/env` is `env`, and reading it as any other program would lose its operands.
/// `builtin`, `command`, and `exec` are shell builtins, so a path named like one is some
/// other program, as is a path named like a builtin that starts no program, such as `./test`.
/// Either may be a project script; read as any other program, it at worst keeps a value.
const PROGRAMS_NAMED_BY_PATH: [&str; 2] = ["env", "nohup"];

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

/// How a command word is found, which decides whether it can name a builtin.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lookup {
    /// The shell finds it, so a builtin it names runs in the shell itself.
    Shell,
    /// A program executes it from `PATH`, as `exec`, `env`, and `nohup` do.
    Program,
}

/// The names a nested launch started by this simple command, or by a later command in the
/// same shell, sees assigned by the command text.
fn names_reaching_a_launch(words: &[String]) -> Vec<String> {
    let reserved = words.iter().take_while(|word| RESERVED_WORDS.contains(&word.as_str())).count();
    let words = &words[reserved..];
    let prefixes = words.iter().take_while(|word| assignment_name(word).is_some()).count();
    let (prefix_words, command) = words.split_at(prefixes);
    let command: Vec<Cow<'_, str>> = command.iter().map(|word| unquoted(word)).collect();
    match command_effect(&command, Lookup::Shell) {
        Effect::NoProgram => Vec::new(),
        Effect::Assigns(names) => {
            assignment_names(prefix_words.iter().map(String::as_str)).chain(names).collect()
        }
    }
}

/// The effect of `command`, the already unquoted words after any prefix assignments, when
/// `lookup` finds its first word. With no words, the prefix assignments are kept: a bare
/// assignment statement, like one on `exec` alone, assigns for the rest of the shell. Any
/// program other than a known builtin may be, or may start, a nested `bento`.
fn command_effect(command: &[Cow<'_, str>], lookup: Lookup) -> Effect {
    let Some((word, arguments)) = command.split_first() else {
        return Effect::Assigns(Vec::new());
    };
    let name = utility_name(word);
    if lookup == Lookup::Shell && EXPORTING_BUILTINS.contains(&name) {
        let operands = arguments.iter().filter(|word| !word.starts_with('-'));
        return Effect::Assigns(assignment_names(operands.map(Cow::as_ref)).collect());
    }
    if NON_LAUNCHING_BUILTINS.contains(&name) || (name == "command" && only_describes(arguments)) {
        return Effect::NoProgram;
    }
    if SHELL_WRAPPERS.contains(&name) {
        return command_effect(skip_options(arguments), lookup);
    }
    if PROGRAM_WRAPPERS.contains(&name) {
        return command_effect(skip_options(arguments), Lookup::Program);
    }
    if name == "env" {
        return env_effect(arguments);
    }
    Effect::Assigns(Vec::new())
}

/// The name of the utility the command word `word` runs: a path to one of
/// [`PROGRAMS_NAMED_BY_PATH`] runs that utility, and any other word stands for itself.
fn utility_name(word: &str) -> &str {
    match word.rsplit_once('/') {
        Some((_, file_name)) if PROGRAMS_NAMED_BY_PATH.contains(&file_name) => file_name,
        _ => word,
    }
}

/// `env`'s `NAME=value` operands apply to the utility it runs, which it finds on `PATH`.
/// Without a utility it prints the environment, and they reach nothing.
fn env_effect(arguments: &[Cow<'_, str>]) -> Effect {
    let operands = env_arguments::operands(arguments);
    let assignments = operands.iter().take_while(|word| assignment_name(word).is_some()).count();
    let (assignment_words, utility) = operands.split_at(assignments);
    if utility.is_empty() {
        return Effect::NoProgram;
    }
    match command_effect(utility, Lookup::Program) {
        Effect::NoProgram => Effect::NoProgram,
        Effect::Assigns(names) => {
            let assigned = assignment_names(assignment_words.iter().map(Cow::as_ref));
            Effect::Assigns(assigned.chain(names).collect())
        }
    }
}

/// Whether `command`'s options in `arguments` include `-v` or `-V`, with which it describes
/// the command that follows instead of running it.
fn only_describes(arguments: &[Cow<'_, str>]) -> bool {
    let mut options = arguments.iter().take_while(|word| word.starts_with('-'));
    options.any(|option| option.contains(['v', 'V']))
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
