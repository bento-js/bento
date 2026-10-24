//! How `bento` reports a script's failure: the kind of failure, the runtime's message as
//! plain text, and the place in the source it points at.

use bento_runtime::{ErrorKind, RuntimeError};

use crate::diagnostic::Diagnostic;

/// `error` as a report of the place in a source file it points at, if it points at one.
pub(crate) fn diagnostic(error: &RuntimeError) -> Option<Diagnostic<'_>> {
    let location = error.location()?;
    let message = DividedMessage::new(&error.message);
    Some(Diagnostic {
        message: format!("{}: {}", describe(error.kind), message.summary),
        path: &location.path,
        line: usize::try_from(location.line).ok()?,
        column: usize::try_from(location.column).ok()?,
        label: label(error.kind).to_owned(),
        trace: message.trace,
    })
}

pub(crate) fn describe(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::Startup => "the script could not start",
        ErrorKind::Resolution => "a module could not be resolved",
        ErrorKind::Transform => "a module could not be transformed",
        ErrorKind::Permission => "permission denied",
        ErrorKind::Uncaught => "uncaught exception",
    }
}

fn label(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::Startup => "not valid here",
        ErrorKind::Resolution => "imported here",
        ErrorKind::Transform => "cannot be transformed here",
        ErrorKind::Permission => "denied here",
        ErrorKind::Uncaught => "thrown here",
    }
}

/// `text` without terminal styling. The runtime styles parts of its messages for terminals
/// whatever the output, and `bento` decides itself when its reports are styled.
pub(crate) fn plain_text(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.next_if_eq(&'[').is_some() {
            characters.by_ref().find(|code| ('@'..='~').contains(code));
        } else {
            plain.push(character);
        }
    }
    plain
}

/// A runtime error's message divided around its stack trace, for a report that quotes the
/// source itself.
struct DividedMessage {
    /// What went wrong, without the runtime's own quote of the source line.
    summary: String,
    /// The stack trace and the errors that caused this one, as the runtime wrote them.
    trace: String,
}

impl DividedMessage {
    fn new(message: &str) -> DividedMessage {
        let plain = plain_text(message);
        let lines: Vec<&str> = plain.lines().collect();
        let trace_start = lines.iter().position(|line| is_stack_frame(line)).unwrap_or(lines.len());
        let (summary, trace) = lines.split_at(trace_start);
        DividedMessage {
            summary: without_source_quote(summary).join("\n"),
            trace: trace.join("\n"),
        }
    }
}

fn is_stack_frame(line: &str) -> bool {
    line.starts_with(char::is_whitespace) && line.trim_start().starts_with("at ")
}

/// `lines` without a quoted source line and the line with a caret under its column.
fn without_source_quote<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    match lines.iter().position(|line| is_caret_line(line)) {
        Some(caret) if caret > 0 => [&lines[..caret - 1], &lines[caret + 1..]].concat(),
        _ => lines.to_vec(),
    }
}

fn is_caret_line(line: &str) -> bool {
    let marker = line.trim();
    !marker.is_empty() && marker.chars().all(|character| character == '^')
}

#[cfg(test)]
mod tests {
    use super::{DividedMessage, plain_text};

    #[test]
    fn styling_is_removed_and_text_kept() {
        let styled =
            "    at \u{1b}[0m\u{1b}[36mfile:///app/throw.ts\u{1b}[0m:\u{1b}[33m6\u{1b}[0m:24";

        assert_eq!(plain_text(styled), "    at file:///app/throw.ts:6:24");
    }

    #[test]
    fn the_quoted_source_line_is_left_out_and_the_stack_trace_follows() {
        let message = "/app/throw.ts: Uncaught Error: no 42\n\
                       const failure: Error = new Error(`no ${answer}`);\n\
                       \u{1b}[31m                       ^\u{1b}[0m\n    \
                       at file:///app/throw.ts:6:24\n    at file:///app/main.ts:1:1";

        let divided = DividedMessage::new(message);

        assert_eq!(divided.summary, "/app/throw.ts: Uncaught Error: no 42");
        assert_eq!(
            divided.trace,
            "    at file:///app/throw.ts:6:24\n    at file:///app/main.ts:1:1"
        );
    }

    #[test]
    fn a_message_of_several_lines_stays_whole() {
        let message = "/app/env.ts: Uncaught AssertionError: \n\nMust provide:\nPORT\n    \
                       at file:///app/env.ts:3:1\nCaused by: Error: missing";

        let divided = DividedMessage::new(message);

        assert_eq!(
            divided.summary,
            "/app/env.ts: Uncaught AssertionError: \n\nMust provide:\nPORT"
        );
        assert_eq!(divided.trace, "    at file:///app/env.ts:3:1\nCaused by: Error: missing");
    }

    #[test]
    fn a_message_without_a_stack_trace_is_all_summary() {
        let message = "/app/main.ts: Cannot resolve \"./absent.ts\" from file:///app/main.ts";

        let divided = DividedMessage::new(message);

        assert_eq!(divided.summary, message);
        assert_eq!(divided.trace, "");
    }
}
