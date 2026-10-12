//! Enough of `sh` word splitting to read a task command's assignments from its text.

mod here_document;

use std::borrow::Cow;
use std::mem;
use std::str::Chars;

use here_document::HereDocument;

/// Splits `command` into the raw words of its simple commands, honoring line continuations,
/// quotes, escapes, comments, redirections, and here-documents, whose bodies are input rather
/// than commands unless they run a command substitution. Unquoted parentheses and backticks
/// also separate commands, so command and arithmetic substitutions count as commands of their
/// own.
pub(crate) fn simple_commands(command: &str) -> Vec<Vec<String>> {
    let mut splitter = Splitter::default();
    let mut characters = CommandText(command.chars());
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                splitter.word.push(character);
                splitter.word.extend(characters.literal().next());
            }
            '\'' | '"' => push_quoted(&mut splitter.word, &mut characters, character),
            '#' if splitter.word.is_empty() => {
                characters.literal().find(|&character| character == '\n');
                splitter.end_line(characters.literal());
            }
            ' ' | '\t' => splitter.end_word(),
            '<' | '>' => {
                let redirection = redirection_operator(character, &mut characters);
                splitter.begin_redirection(redirection);
            }
            '\n' => splitter.end_line(characters.literal()),
            ';' | '&' | '|' | '(' | ')' | '`' => splitter.end_command(),
            _ => splitter.word.push(character),
        }
    }
    splitter.end_command();
    splitter.commands
}

/// What the word after a redirection operator is. Either way, it is not a word of the command.
enum Redirection {
    /// A file or a file descriptor, as in `> out` or `2>&1`.
    Target,
    /// The delimiter of a here-document that `<<` or `<<-` opens.
    HereDocument { strip_tabs: bool },
}

/// Consumes the rest of the redirection operator that `first` begins, such as `>>`, `2>&1`'s
/// `>&`, `<<-`, or bash's here-string `<<<`, and returns what the word after it is.
fn redirection_operator(first: char, characters: &mut CommandText<'_>) -> Redirection {
    let second = if first == '>' { ['>', '&', '|'] } else { ['<', '&', '>'] };
    if characters.next_if(|character| second.contains(character)) != Some('<') {
        return Redirection::Target;
    }
    if characters.next_if(|&character| character == '<').is_some() {
        return Redirection::Target;
    }
    let strip_tabs = characters.next_if(|&character| character == '-').is_some();
    Redirection::HereDocument { strip_tabs }
}

/// Appends a quoted span to `word`, quotes included. Inside single quotes every character is
/// literal; inside double quotes a backslash escapes the next character, and a line
/// continuation is removed.
fn push_quoted(word: &mut String, characters: &mut CommandText<'_>, quote: char) {
    word.push(quote);
    let literal = quote == '\'';
    loop {
        let next = if literal { characters.literal().next() } else { characters.next() };
        let Some(character) = next else {
            return;
        };
        word.push(character);
        if character == quote {
            return;
        }
        if character == '\\' && !literal {
            word.extend(characters.literal().next());
        }
    }
}

/// A command's characters as the shell's token recognition reads them. Outside single quotes,
/// comments, and here-document bodies, the shell removes each line continuation, a backslash
/// followed by a newline, so the characters on either side of it join without a separator.
struct CommandText<'a>(Chars<'a>);

impl<'a> CommandText<'a> {
    /// The rest of the text as written, line continuations included: for a single-quoted span,
    /// a comment, the character a backslash escapes, and a here-document body, which handles
    /// its own.
    fn literal(&mut self) -> &mut Chars<'a> {
        &mut self.0
    }

    /// Consumes the next character, past any line continuations, when `accept` accepts it.
    fn next_if(&mut self, accept: impl FnOnce(&char) -> bool) -> Option<char> {
        self.skip_line_continuations();
        let next = self.0.as_str().chars().next().filter(accept)?;
        self.0.next();
        Some(next)
    }

    fn skip_line_continuations(&mut self) {
        while self.0.as_str().starts_with("\\\n") {
            self.0.nth(1);
        }
    }
}

impl Iterator for CommandText<'_> {
    type Item = char;

    /// The next character, past any line continuations.
    fn next(&mut self) -> Option<char> {
        self.skip_line_continuations();
        self.0.next()
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
    /// What the next word is when a redirection operator precedes it, which makes it no part
    /// of the command.
    pending_redirection: Option<Redirection>,
    /// The here-documents opened on the current line, whose bodies follow its newline.
    here_documents: Vec<HereDocument>,
}

impl Splitter {
    /// Drops the word in progress when it is the redirected file descriptor's number, as in
    /// `2>`, and records what the word that follows is.
    fn begin_redirection(&mut self, redirection: Redirection) {
        if self.word.chars().all(|character| character.is_ascii_digit()) {
            self.word.clear();
        } else {
            self.end_word();
        }
        self.pending_redirection = Some(redirection);
    }

    fn end_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        let word = mem::take(&mut self.word);
        match self.pending_redirection.take() {
            None => self.words.push(word),
            Some(Redirection::Target) => {}
            Some(Redirection::HereDocument { strip_tabs }) => {
                self.here_documents.push(HereDocument::new(&word, strip_tabs));
            }
        }
    }

    fn end_command(&mut self) {
        self.end_word();
        self.pending_redirection = None;
        if !self.words.is_empty() {
            self.commands.push(mem::take(&mut self.words));
        }
    }

    /// Ends the command at a newline, then consumes from `text` the bodies of the
    /// here-documents the line opened, in order, splitting the command text of each body that
    /// runs a command substitution.
    fn end_line(&mut self, text: &mut Chars<'_>) {
        self.end_command();
        for here_document in self.here_documents.drain(..) {
            let Some(body) = here_document.consume_body(text) else {
                continue;
            };
            for command_text in here_document::command_texts(&body) {
                self.commands.extend(simple_commands(command_text));
            }
        }
    }
}
