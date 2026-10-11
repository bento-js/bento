//! Here-documents, whose bodies are their commands' input rather than command text.

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
    /// rest of the text when no line matches the delimiter.
    pub(super) fn skip_body(&self, text: &mut Chars<'_>) {
        let mut line = String::new();
        while !text.as_str().is_empty() {
            self.read_line(text, &mut line);
            let content = if self.strip_tabs { line.trim_start_matches('\t') } else { &line };
            if content == self.delimiter {
                return;
            }
        }
    }

    /// Replaces `line` with the body's next line, consuming its newline. Unless the body is
    /// literal, a line continuation joins the line to the next, which then cannot end the body.
    fn read_line(&self, text: &mut Chars<'_>, line: &mut String) {
        line.clear();
        while let Some(character) = text.next() {
            match character {
                '\n' => return,
                '\\' if !self.literal => match text.next() {
                    Some('\n') => {}
                    escaped => {
                        line.push(character);
                        line.extend(escaped);
                    }
                },
                _ => line.push(character),
            }
        }
    }
}
