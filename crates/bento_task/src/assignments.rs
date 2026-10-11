//! The variables a task command assigns itself. A nested `bento` launch must treat them as
//! explicit even when they repeat a value loaded from a file, and the environment it
//! inherits is identical either way, so only the command text can tell.

use std::collections::BTreeSet;
use std::mem;
use std::str::Chars;

/// Words that may precede a command without being its name.
const RESERVED_WORDS: [&str; 9] = ["!", "{", "if", "then", "elif", "else", "while", "until", "do"];

/// Commands whose `NAME=value` arguments assign variables.
const ASSIGNING_COMMANDS: [&str; 2] = ["export", "readonly"];

/// Names assigned by `NAME=value` words before a command and by the `NAME=value`
/// arguments of `export`, `readonly`, and `env`, in any simple command of `command`.
pub(crate) fn assigned_names(command: &str) -> BTreeSet<String> {
    simple_commands(command)
        .iter()
        .flat_map(|words| assignments_in(words))
        .map(str::to_owned)
        .collect()
}

fn assignments_in(words: &[String]) -> Vec<&str> {
    let mut words = words
        .iter()
        .map(String::as_str)
        .skip_while(|word| RESERVED_WORDS.contains(word))
        .peekable();
    let mut names = Vec::new();
    while let Some(name) = words.peek().and_then(|word| assigned_name(word)) {
        names.push(name);
        words.next();
    }
    match words.next() {
        Some(command) if ASSIGNING_COMMANDS.contains(&command) => {
            names.extend(words.filter_map(assigned_name));
        }
        Some("env") => {
            names.extend(words.filter(|word| !word.starts_with('-')).map_while(assigned_name));
        }
        _ => {}
    }
    names
}

/// The name a word assigns. Quotes are kept in the words, so a quoted name never matches.
fn assigned_name(word: &str) -> Option<&str> {
    let (name, _) = word.split_once('=')?;
    let mut characters = name.chars();
    let starts_a_name =
        characters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    let is_name = starts_a_name
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
    is_name.then_some(name)
}

/// Splits `command` into the raw words of its simple commands, honoring quotes, escapes,
/// and comments. Unquoted parentheses and backticks also separate commands, so command
/// and arithmetic substitutions count as commands of their own.
fn simple_commands(command: &str) -> Vec<Vec<String>> {
    let mut splitter = Splitter::default();
    let mut characters = command.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                splitter.word.push(character);
                splitter.word.extend(characters.next());
            }
            '\'' | '"' => push_quoted(&mut splitter.word, &mut characters, character),
            '#' if splitter.word.is_empty() => {
                characters.by_ref().find(|&character| character == '\n');
                splitter.end_command();
            }
            ' ' | '\t' => splitter.end_word(),
            '\n' | ';' | '&' | '|' | '(' | ')' | '`' => splitter.end_command(),
            _ => splitter.word.push(character),
        }
    }
    splitter.end_command();
    splitter.commands
}

/// Appends a quoted span to `word`, quotes included.
fn push_quoted(word: &mut String, characters: &mut Chars<'_>, quote: char) {
    word.push(quote);
    while let Some(character) = characters.next() {
        word.push(character);
        if character == quote {
            return;
        }
        if character == '\\' && quote == '"' {
            word.extend(characters.next());
        }
    }
}

#[derive(Default)]
struct Splitter {
    commands: Vec<Vec<String>>,
    words: Vec<String>,
    word: String,
}

impl Splitter {
    fn end_word(&mut self) {
        if !self.word.is_empty() {
            self.words.push(mem::take(&mut self.word));
        }
    }

    fn end_command(&mut self) {
        self.end_word();
        if !self.words.is_empty() {
            self.commands.push(mem::take(&mut self.words));
        }
    }
}
