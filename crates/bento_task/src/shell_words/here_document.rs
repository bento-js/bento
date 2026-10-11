//! Here-documents, whose bodies are their commands' input rather than command text, unless
//! the shell runs a command substitution from one.

use std::iter;
use std::str::Chars;

use super::unquoted;

/// A here-document that `<<` or `<<-` opened. Its body begins after the next newline of the
/// command text.
pub(super) struct HereDocument {
    delimiter: String,
    /// Some part of the delimiter word was quoted, so the shell leaves the body as written,
    /// line continuations included.
    literal: bool,
    /// `<<-` removes the leading tabs from each line, the delimiter line's included.
    strip_tabs: bool,
}

impl HereDocument {
    /// The here-document whose raw delimiter word is `word`.
    pub(super) fn new(word: &str, strip_tabs: bool) -> Self {
        Self {
            delimiter: unquoted(word).into_owned(),
            literal: word.contains(['\\', '\'', '"']),
            strip_tabs,
        }
    }

    /// Consumes the body that `text` begins with: its lines through the delimiter line, or the
    /// rest of the text when no line matches the delimiter. Returns the body as the shell reads
    /// it, line continuations joined and without the delimiter line, when the body is not
    /// literal and runs a command substitution.
    pub(super) fn consume_body(&self, text: &mut Chars<'_>) -> Option<String> {
        let mut body = String::new();
        while !text.as_str().is_empty() {
            let line_start = body.len();
            self.read_line(text, &mut body);
            let line = &body[line_start..];
            let content = if self.strip_tabs { line.trim_start_matches('\t') } else { line };
            if content == self.delimiter {
                body.truncate(line_start);
                break;
            }
            body.push('\n');
        }
        let runs_commands = !self.literal && substitution_texts(&body).next().is_some();
        runs_commands.then_some(body)
    }

    /// Appends the body's next line to `body`, consuming its newline. Unless the body is
    /// literal, a line continuation joins the line to the next, which then cannot end the body.
    fn read_line(&self, text: &mut Chars<'_>, body: &mut String) {
        while let Some(character) = text.next() {
            match character {
                '\n' => return,
                '\\' if !self.literal => match text.next() {
                    Some('\n') => {}
                    escaped => {
                        body.push(character);
                        body.extend(escaped);
                    }
                },
                _ => body.push(character),
            }
        }
    }
}

/// The command text of a body that runs a command substitution. Each line counts, keeping the
/// user's value for an assignment the shell may not run. Bento does not find where a
/// substitution ends, so the text from each one to the body's end counts too: the body's
/// quotes and `#` are only its text and do not hide the substitutions after them.
pub(super) fn command_texts(body: &str) -> impl Iterator<Item = &str> {
    body.lines().chain(substitution_texts(body))
}

/// The text from each command substitution in a body to the body's end: after each `$(` and
/// each backtick that no backslash escapes. `$((` counts, because bash runs `$((…) …)` as a
/// substitution of a subshell rather than an arithmetic expansion.
fn substitution_texts(body: &str) -> impl Iterator<Item = &str> {
    let mut characters = body.char_indices().peekable();
    iter::from_fn(move || {
        while let Some((index, character)) = characters.next() {
            match character {
                '\\' => {
                    characters.next();
                }
                '`' => return Some(&body[index + 1..]),
                '$' => {
                    if let Some((open, _)) = characters.next_if(|&(_, next)| next == '(') {
                        return Some(&body[open + 1..]);
                    }
                }
                _ => {}
            }
        }
        None
    })
}
