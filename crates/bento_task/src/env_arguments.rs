//! The operands of an `env` command: the words after its options, with each `-S` string
//! split into words the way `env` splits it and processed in its place.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::mem;
use std::str::Chars;

/// Short options whose argument is the rest of the option word or, when that is empty, the
/// next word.
const SHORT_OPTIONS_WITH_ARGUMENT: [char; 5] = ['a', 'C', 'P', 'S', 'u'];
const LONG_OPTIONS_WITH_ARGUMENT: [&str; 4] = ["argv0", "chdir", "split-string", "unset"];

/// The words after `env`'s options. `env` processes the words of a `-S` string in its place,
/// so they may hold further options as well as the assignments and the utility.
pub(crate) fn operands<'a>(arguments: &[Cow<'a, str>]) -> Vec<Cow<'a, str>> {
    let mut words: VecDeque<Cow<'a, str>> = arguments.iter().cloned().collect();
    while let Some(word) = words.pop_front() {
        if word == "--" {
            break;
        }
        if !word.starts_with('-') {
            words.push_front(word);
            break;
        }
        let Some(argument) = option_argument(&word) else {
            continue;
        };
        let value = match argument.attached {
            Some(attached) => Some(Cow::Borrowed(attached)),
            None => words.pop_front(),
        };
        if let Some(string) = value.filter(|_| argument.splits) {
            for split in split_string(&string).into_iter().rev() {
                words.push_front(Cow::Owned(split));
            }
        }
    }
    words.into()
}

/// The argument of an option that takes one.
struct OptionArgument<'w> {
    /// `-S` or `--split-string`: `env` splits the argument into words.
    splits: bool,
    /// The argument when it is part of the option word; otherwise it is the next word.
    attached: Option<&'w str>,
}

fn option_argument(word: &str) -> Option<OptionArgument<'_>> {
    if let Some(long) = word.strip_prefix("--") {
        let (name, attached) = match long.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (long, None),
        };
        let splits = name == "split-string";
        return LONG_OPTIONS_WITH_ARGUMENT
            .contains(&name)
            .then_some(OptionArgument { splits, attached });
    }
    let mut letters = word.char_indices().skip(1);
    let (index, letter) =
        letters.find(|(_, letter)| SHORT_OPTIONS_WITH_ARGUMENT.contains(letter))?;
    let attached = &word[index + letter.len_utf8()..];
    let attached = (!attached.is_empty()).then_some(attached);
    Some(OptionArgument { splits: letter == 'S', attached })
}

/// Splits a `-S` string as `env` does: space, tab, and an unquoted `\_` separate words;
/// quotes and backslashes escape them; a word starting with an unquoted `#`, and an unquoted
/// `\c`, end the string.
fn split_string(string: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut characters = string.chars();
    while let Some(character) = characters.next() {
        match character {
            ' ' | '\t' => end_word(&mut words, &mut word),
            '#' if word.is_empty() => break,
            '\'' => push_single_quoted(&mut word, &mut characters),
            '"' => push_double_quoted(&mut word, &mut characters),
            '\\' => match characters.next() {
                Some('_') => end_word(&mut words, &mut word),
                Some('c') | None => break,
                Some(escaped) => word.push(escaped_character(escaped)),
            },
            _ => word.push(character),
        }
    }
    end_word(&mut words, &mut word);
    words
}

fn end_word(words: &mut Vec<String>, word: &mut String) {
    if !word.is_empty() {
        words.push(mem::take(word));
    }
}

/// Inside single quotes only `\'` and `\\` are escapes.
fn push_single_quoted(word: &mut String, characters: &mut Chars<'_>) {
    while let Some(character) = characters.next() {
        match character {
            '\'' => return,
            '\\' => match characters.next() {
                Some(escaped @ ('\'' | '\\')) => word.push(escaped),
                Some(kept) => {
                    word.push('\\');
                    word.push(kept);
                }
                None => word.push('\\'),
            },
            _ => word.push(character),
        }
    }
}

/// Inside double quotes every escape applies, and `\_` is a space rather than a separator.
fn push_double_quoted(word: &mut String, characters: &mut Chars<'_>) {
    while let Some(character) = characters.next() {
        match character {
            '"' => return,
            '\\' => match characters.next() {
                Some('_') => word.push(' '),
                Some(escaped) => word.push(escaped_character(escaped)),
                None => word.push('\\'),
            },
            _ => word.push(character),
        }
    }
}

/// The character a `-S` backslash escape stands for. `#`, `$`, quotes, and `\` stand for
/// themselves; `env` rejects any other letter, and the word it would form is no name either.
fn escaped_character(escaped: char) -> char {
    match escaped {
        'f' => '\x0c',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'v' => '\x0b',
        other => other,
    }
}
