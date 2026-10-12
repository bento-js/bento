use std::path::{Path, PathBuf};

use thiserror::Error;

/// A malformed environment file. Carries the location, never the offending value.
#[derive(Debug, Error)]
#[error("{}:{line}: {problem}", .path.display())]
pub struct DotenvError {
    pub path: PathBuf,
    pub line: usize,
    pub problem: DotenvProblem,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum DotenvProblem {
    #[error("expected `NAME=value`")]
    MissingAssignment,
    #[error("variable names may contain only letters, digits, `_`, `.`, and `-`")]
    InvalidName,
    #[error("quoted value is never closed")]
    UnterminatedQuote,
    #[error("unexpected text after the closing quote")]
    TextAfterQuote,
}

/// Parses dotenv `source` into assignments in file order. Values are taken literally:
/// nothing is expanded or executed.
pub(crate) fn parse(path: &Path, source: &str) -> Result<Vec<(String, String)>, DotenvError> {
    let normalized = source.replace("\r\n", "\n");
    let mut lines = normalized.split('\n').enumerate();
    let mut assignments = Vec::new();
    while let Some((index, line)) = lines.next() {
        let statement = line.trim_start();
        if statement.is_empty() || statement.starts_with('#') {
            continue;
        }
        let fail = |problem| DotenvError { path: path.to_owned(), line: index + 1, problem };
        let (name, rest) = split_assignment(statement).map_err(fail)?;
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(quote @ ('"' | '\'' | '`')) => {
                quoted_value(quote, &rest[1..], lines.by_ref().map(|(_, line)| line))
                    .map_err(fail)?
            }
            _ => unquoted_value(rest),
        };
        assignments.push((name.to_owned(), value));
    }
    Ok(assignments)
}

fn split_assignment(statement: &str) -> Result<(&str, &str), DotenvProblem> {
    let statement = strip_export(statement);
    let (name, rest) = statement.split_once('=').ok_or(DotenvProblem::MissingAssignment)?;
    let name = name.trim_end();
    let valid = !name.is_empty()
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte));
    if valid { Ok((name, rest)) } else { Err(DotenvProblem::InvalidName) }
}

fn strip_export(statement: &str) -> &str {
    match statement.strip_prefix("export") {
        Some(rest) if rest.starts_with([' ', '\t']) => rest.trim_start(),
        _ => statement,
    }
}

/// An unquoted value ends at the first `#`, matching Node's `--env-file` parser.
fn unquoted_value(rest: &str) -> String {
    let value = rest.split_once('#').map_or(rest, |(value, _comment)| value);
    value.trim().to_owned()
}

/// Reads a quoted value that starts in `opened` and may continue onto following lines.
fn quoted_value<'a>(
    quote: char,
    opened: &str,
    mut following: impl Iterator<Item = &'a str>,
) -> Result<String, DotenvProblem> {
    let mut text = opened.to_owned();
    let closing = loop {
        if let Some(closing) = closing_quote(&text, quote) {
            break closing;
        }
        let next = following.next().ok_or(DotenvProblem::UnterminatedQuote)?;
        text.push('\n');
        text.push_str(next);
    };
    let after = text[closing + 1..].trim_start();
    if !after.is_empty() && !after.starts_with('#') {
        return Err(DotenvProblem::TextAfterQuote);
    }
    let content = &text[..closing];
    Ok(if quote == '"' { unescape(content) } else { content.to_owned() })
}

/// The byte index of the quote that closes `text`. Only double quotes have escapes.
fn closing_quote(text: &str, quote: char) -> Option<usize> {
    let mut characters = text.char_indices();
    while let Some((index, character)) = characters.next() {
        if character == quote {
            return Some(index);
        }
        if character == '\\' && quote == '"' {
            characters.next();
        }
    }
    None
}

fn unescape(content: &str) -> String {
    let mut value = String::with_capacity(content.len());
    let mut characters = content.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            value.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => value.push('\n'),
            Some('r') => value.push('\r'),
            Some('t') => value.push('\t'),
            Some(escaped @ ('"' | '\\')) => value.push(escaped),
            Some(other) => {
                value.push('\\');
                value.push(other);
            }
            None => value.push('\\'),
        }
    }
    value
}
