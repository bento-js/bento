//! Enough of `sh` word splitting to read a task command's assignments from its text.

use std::borrow::Cow;
use std::iter::Peekable;
use std::mem;
use std::str::Chars;

/// Splits `command` into the raw words of its simple commands, honoring quotes, escapes,
/// comments, and redirections. Unquoted parentheses and backticks also separate commands,
/// so command and arithmetic substitutions count as commands of their own.
pub(crate) fn simple_commands(command: &str) -> Vec<Vec<String>> {
    let mut splitter = Splitter::default();
    let mut characters = command.chars().peekable();
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
            '<' | '>' => {
                skip_redirection_operator(character, &mut characters);
                splitter.begin_redirection();
            }
            '\n' | ';' | '&' | '|' | '(' | ')' | '`' => splitter.end_command(),
            _ => splitter.word.push(character),
        }
    }
    splitter.end_command();
    splitter.commands
}

/// Consumes the rest of the redirection operator that `first` begins, such as `>>`, `2>&1`'s
/// `>&`, or `<<-`.
fn skip_redirection_operator(first: char, characters: &mut Peekable<Chars<'_>>) {
    let second = if first == '>' { ['>', '&', '|'] } else { ['<', '&', '>'] };
    if characters.next_if(|character| second.contains(character)) == Some('<') {
        characters.next_if_eq(&'-');
    }
}

/// Appends a quoted span to `word`, quotes included.
fn push_quoted(word: &mut String, characters: &mut Peekable<Chars<'_>>, quote: char) {
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

/// The text a raw word yields after quote removal, as the program receiving it sees it.
pub(crate) fn unquoted(word: &str) -> Cow<'_, str> {
    if !word.contains(['\\', '\'', '"']) {
        return Cow::Borrowed(word);
    }
    let mut unquoted = String::with_capacity(word.len());
    let mut characters = word.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => unquoted.extend(characters.next()),
            '\'' => unquoted.extend(characters.by_ref().take_while(|&character| character != '\'')),
            '"' => push_double_quoted(&mut unquoted, &mut characters),
            _ => unquoted.push(character),
        }
    }
    Cow::Owned(unquoted)
}

fn push_double_quoted(unquoted: &mut String, characters: &mut Chars<'_>) {
    while let Some(character) = characters.next() {
        match character {
            '"' => return,
            '\\' => match characters.next() {
                Some(escaped @ ('"' | '\\' | '$' | '`')) => unquoted.push(escaped),
                Some(kept) => {
                    unquoted.push('\\');
                    unquoted.push(kept);
                }
                None => unquoted.push('\\'),
            },
            _ => unquoted.push(character),
        }
    }
}

#[derive(Default)]
struct Splitter {
    commands: Vec<Vec<String>>,
    words: Vec<String>,
    word: String,
    /// The next word is a redirection's target, not part of the command.
    redirection_target_pending: bool,
}

impl Splitter {
    /// Drops the word in progress when it is the redirected file descriptor's number, as in
    /// `2>`, and marks the word that follows as the target.
    fn begin_redirection(&mut self) {
        if self.word.chars().all(|character| character.is_ascii_digit()) {
            self.word.clear();
        } else {
            self.end_word();
        }
        self.redirection_target_pending = true;
    }

    fn end_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        let word = mem::take(&mut self.word);
        if !mem::take(&mut self.redirection_target_pending) {
            self.words.push(word);
        }
    }

    fn end_command(&mut self) {
        self.end_word();
        self.redirection_target_pending = false;
        if !self.words.is_empty() {
            self.commands.push(mem::take(&mut self.words));
        }
    }
}
