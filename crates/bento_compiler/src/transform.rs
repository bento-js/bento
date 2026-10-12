use std::path::{Path, PathBuf};

use oxc::allocator::Allocator;
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::diagnostics::{Diagnostics, OxcDiagnostic, Severity};
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use oxc::transformer::Transformer;
use thiserror::Error;

use crate::position::line_column;
use crate::settings::{CompilerSettings, JsxMode};
use crate::syntax_survey::SyntaxSurvey;
use crate::transform_options::transform_options;

/// JavaScript the runtime evaluates in place of a TypeScript, TSX, or JSX source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transformed {
    pub code: String,
    /// Source map v3 JSON whose single source is the input path.
    pub source_map: String,
}

/// A source file could not be turned into executable JavaScript.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("{}:{line}:{column}: {message}", path.display())]
pub struct TransformError {
    pub path: PathBuf,
    /// One-based line and column of the offending syntax.
    pub line: u32,
    pub column: u32,
    pub message: String,
}

const TRANSFORMED_EXTENSIONS: &[&str] = &["ts", "mts", "cts", "tsx", "jsx"];

/// Removes type syntax and compiles JSX in `source`, read from `path`, according to
/// `settings`. Module syntax is kept as written; nothing is downleveled or type checked.
pub fn transform(
    path: &Path,
    source: &str,
    settings: &CompilerSettings,
) -> Result<Transformed, TransformError> {
    let source_type = source_type_of(path)?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if let Some(error) = first_error(&parsed.diagnostics) {
        return Err(TransformError::from_diagnostic(path, source, error));
    }
    let mut program = parsed.program;
    ensure_executable(path, source, settings, &SyntaxSurvey::of(&program))?;

    let semantic = SemanticBuilder::new_compiler()
        .with_excess_capacity(2.0)
        .with_enum_eval(true)
        .build(&program);
    if let Some(error) = first_error(&semantic.diagnostics) {
        return Err(TransformError::from_diagnostic(path, source, error));
    }
    let scoping = semantic.semantic.into_scoping();

    let options = transform_options(settings);
    let transformed =
        Transformer::new(&allocator, path, &options).build_with_scoping(scoping, &mut program);
    if let Some(error) = first_error(&transformed.diagnostics) {
        return Err(TransformError::from_diagnostic(path, source, error));
    }

    let codegen_options =
        CodegenOptions { source_map_path: Some(path.to_path_buf()), ..CodegenOptions::default() };
    let generated = Codegen::new().with_options(codegen_options).build(&program);
    let source_map = generated.map.expect("codegen was given a source map path").to_json_string();
    Ok(Transformed { code: generated.code, source_map })
}

fn source_type_of(path: &Path) -> Result<SourceType, TransformError> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    match extension {
        Some(extension) if TRANSFORMED_EXTENSIONS.contains(&extension) => {
            SourceType::from_extension(extension).map_err(|error| TransformError {
                path: path.to_path_buf(),
                line: 1,
                column: 1,
                message: error.to_string(),
            })
        }
        _ => Err(TransformError {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            message: format!(
                "`{}` is not a TypeScript or JSX file; Bento transforms .ts, .mts, .cts, .tsx, and .jsx",
                path.display()
            ),
        }),
    }
}

/// Fails on syntax that no engine runs unless a setting selects its transform.
fn ensure_executable(
    path: &Path,
    source: &str,
    settings: &CompilerSettings,
    survey: &SyntaxSurvey,
) -> Result<(), TransformError> {
    if let JsxMode::Unavailable { configured } = &settings.jsx
        && let Some(span) = survey.first_jsx
    {
        let message = jsx_unavailable(configured);
        return Err(TransformError::at(path, source, span.start as usize, message));
    }
    if !settings.experimental_decorators
        && let Some(span) = survey.first_decorator
    {
        return Err(TransformError::at(
            path,
            source,
            span.start as usize,
            "decorators run only as TypeScript legacy decorators; set compilerOptions \"experimentalDecorators\": true in tsconfig.json (standard decorators are not transformed)".to_owned(),
        ));
    }
    Ok(())
}

fn jsx_unavailable(configured: &Option<String>) -> String {
    const CHOICES: &str = "set \"jsx\" to \"react-jsx\" (with \"jsxImportSource\"), \"react-jsxdev\", or \"react\" in tsconfig.json";
    match configured {
        Some(configured) => format!(
            "compilerOptions \"jsx\": \"{configured}\" leaves JSX untransformed, which cannot run; {CHOICES}"
        ),
        None => format!("this file contains JSX but compilerOptions \"jsx\" is not set; {CHOICES}"),
    }
}

fn first_error(diagnostics: &Diagnostics) -> Option<&OxcDiagnostic> {
    diagnostics.into_iter().find(|diagnostic| diagnostic.severity == Severity::Error)
}

impl TransformError {
    fn from_diagnostic(path: &Path, source: &str, diagnostic: &OxcDiagnostic) -> Self {
        let offset = diagnostic.labels.first().map_or(0, |label| label.offset() as usize);
        Self::at(path, source, offset, diagnostic.message.to_string())
    }

    fn at(path: &Path, source: &str, offset: usize, message: String) -> Self {
        let (line, column) = line_column(source, offset);
        Self { path: path.to_path_buf(), line, column, message }
    }
}
