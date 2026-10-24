//! Showing an error at the place in a source file it points at.

use std::ffi::OsStr;
use std::fs;
use std::ops::Range;
use std::path::Path;

use ariadne::{Config, Label, Report, ReportKind, Source};

const ENVIRONMENT_FILE: &str = ".env";

/// An error that points at a place in a source file.
pub(crate) struct Diagnostic<'a> {
    pub message: String,
    pub path: &'a Path,
    /// One-based.
    pub line: usize,
    /// One-based, counted in characters.
    pub column: usize,
    /// What is wrong at that place.
    pub label: String,
    /// Shown after the quoted source as it is, such as a stack trace; may be empty.
    pub trace: String,
}

/// `diagnostic` as a report quoting the source around its place, styled with colors when
/// `color` is set. `None` when that source cannot be shown: the file cannot be read, has no
/// such line, or is an environment file, whose values are often secrets.
pub(crate) fn render(diagnostic: &Diagnostic, color: bool) -> Option<String> {
    if is_environment_file(diagnostic.path) {
        return None;
    }
    let source = fs::read_to_string(diagnostic.path).ok()?;
    render_source(diagnostic, &source, color)
}

fn render_source(diagnostic: &Diagnostic, source: &str, color: bool) -> Option<String> {
    let span = marked_span(source, diagnostic.line, diagnostic.column)?;
    let name = diagnostic.path.display().to_string();
    let mut report = Vec::new();
    Report::build(ReportKind::Error, (name.clone(), span.clone()))
        .with_config(Config::default().with_color(color))
        .with_message(&diagnostic.message)
        .with_label(Label::new((name.clone(), span)).with_message(&diagnostic.label))
        .finish()
        .write((name, Source::from(source)), &mut report)
        .ok()?;
    let mut report = String::from_utf8(report).ok()?;
    if !diagnostic.trace.is_empty() {
        report.push_str(&diagnostic.trace);
        report.push('\n');
    }
    Some(report)
}

/// The characters to mark for one-based `line` and `column` in `source`: the character there,
/// or the end of the line when the column is past it. `None` when `source` has no such line.
fn marked_span(source: &str, line: usize, column: usize) -> Option<Range<usize>> {
    let mut lines = source.split('\n');
    let preceding = lines.by_ref().take(line.checked_sub(1)?);
    let line_start: usize = preceding.map(|preceding| preceding.chars().count() + 1).sum();
    let line_length = lines.next()?.chars().count();
    let within_line = column.saturating_sub(1);
    if within_line >= line_length {
        return Some(line_start + line_length..line_start + line_length);
    }
    Some(line_start + within_line..line_start + within_line + 1)
}

fn is_environment_file(path: &Path) -> bool {
    path.file_name().and_then(OsStr::to_str).is_some_and(|name| {
        name.strip_prefix(ENVIRONMENT_FILE)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::{Diagnostic, render};

    const SCRATCH: &str = "/tmp/bento-cli/diagnostic";
    const SOURCE: &str = "const ready = true;\nthrow new Error('kaboom');\nconsole.log(ready);\n";

    fn scratch() -> TempDir {
        fs::create_dir_all(SCRATCH).unwrap();
        tempfile::tempdir_in(SCRATCH).unwrap()
    }

    fn write(directory: &TempDir, name: &str, contents: &str) -> PathBuf {
        let path = directory.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn thrown_at(path: &Path, line: usize, column: usize) -> Diagnostic<'_> {
        Diagnostic {
            message: "uncaught exception: Error: kaboom".to_owned(),
            path,
            line,
            column,
            label: "thrown here".to_owned(),
            trace: String::new(),
        }
    }

    #[test]
    fn a_report_quotes_the_line_and_marks_the_column() {
        let directory = scratch();
        let path = write(&directory, "boom.mjs", SOURCE);

        let report = render(&thrown_at(&path, 2, 7), false).expect("the file can be shown");

        assert!(report.contains("uncaught exception: Error: kaboom"), "{report}");
        assert!(report.contains(&format!("{}:2:7", path.display())), "{report}");
        assert!(report.contains("throw new Error('kaboom');"), "{report}");
        assert!(report.contains("thrown here"), "{report}");
        let marker_line = report.lines().find(|line| line.contains('┬')).expect("a marker");
        let source_line = report.lines().find(|line| line.contains("throw new")).unwrap();
        assert_eq!(marker_line.find('┬'), source_line.find("new"), "{report}");
    }

    #[test]
    fn a_trace_follows_the_quoted_source() {
        let directory = scratch();
        let path = write(&directory, "boom.mjs", SOURCE);
        let trace = "    at file:///boom.mjs:2:7\n    at file:///main.mjs:1:1";

        let diagnostic = Diagnostic { trace: trace.to_owned(), ..thrown_at(&path, 2, 7) };
        let report = render(&diagnostic, false).expect("the file can be shown");

        assert!(report.ends_with(&format!("╯\n{trace}\n")), "{report}");
    }

    #[test]
    fn colors_are_used_only_when_asked_for() {
        let directory = scratch();
        let path = write(&directory, "boom.mjs", SOURCE);

        let plain = render(&thrown_at(&path, 2, 7), false).unwrap();
        let colored = render(&thrown_at(&path, 2, 7), true).unwrap();

        assert!(!plain.contains('\u{1b}'), "{plain}");
        assert!(colored.contains('\u{1b}'), "{colored}");
    }

    #[test]
    fn a_column_past_the_end_of_its_line_marks_the_line_s_end() {
        let directory = scratch();
        let path = write(&directory, "boom.mjs", SOURCE);

        let report = render(&thrown_at(&path, 1, 80), false).expect("the file can be shown");

        assert!(report.contains("const ready = true;"), "{report}");
    }

    #[test]
    fn a_file_that_cannot_be_read_or_lacks_the_line_is_not_shown() {
        let directory = scratch();
        let path = write(&directory, "boom.mjs", SOURCE);
        let missing = directory.path().join("missing.mjs");

        assert!(render(&thrown_at(&missing, 1, 1), false).is_none());
        assert!(render(&thrown_at(&path, 9, 1), false).is_none());
        assert!(render(&thrown_at(&path, 0, 1), false).is_none());
    }

    #[test]
    fn environment_files_are_never_shown() {
        let directory = scratch();
        for name in [".env", ".env.local", ".env.development"] {
            let path = write(&directory, name, "SESSION_SECRET=\"hunter2\n");

            assert!(render(&thrown_at(&path, 1, 16), false).is_none(), "{name}");
        }
    }
}
