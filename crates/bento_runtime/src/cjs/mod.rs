mod exports;

use std::borrow::Cow;
use std::sync::Arc;

use deno_core::url::Url;
use deno_error::JsErrorBox;
use deno_media_type::MediaType;
use deno_runtime::deno_permissions::{OpenAccessKind, PermissionsContainer};
use node_resolver::analyze::{CjsAnalysis, CjsAnalysisExports, EsmAnalysisMode};
use oxc::allocator::Allocator;
use oxc::ast::ast::Statement;
use oxc::ast_visit::Visit;
use oxc::parser::Parser;
use oxc::span::SourceType;

use crate::cjs::exports::Exports;
use crate::node::CjsTracker;
use crate::sources::Sources;
use crate::{ErrorKind, SourceLocation};

pub(crate) struct Analyzer {
    pub tracker: Arc<CjsTracker>,
    pub permissions: PermissionsContainer,
    pub sources: Arc<Sources>,
}

#[async_trait::async_trait(?Send)]
impl node_resolver::analyze::CjsCodeAnalyzer for Analyzer {
    async fn analyze_cjs<'a>(
        &self,
        specifier: &Url,
        source: Option<Cow<'a, str>>,
        _mode: EsmAnalysisMode,
    ) -> Result<CjsAnalysis<'a>, JsErrorBox> {
        if is_json_or_addon(specifier) {
            return Ok(CjsAnalysis::Cjs(CjsAnalysisExports {
                exports: Vec::new(),
                reexports: Vec::new(),
                member_reexports: Vec::new(),
            }));
        }
        let source =
            source.map(Ok).unwrap_or_else(|| read(specifier, &self.permissions).map(Cow::Owned))?;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::unambiguous()).parse();
        if let Some(error) = parsed.diagnostics.first() {
            let offset = error.labels.first().map_or(0, |label| label.offset() as usize);
            let prefix = &source[..offset];
            let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
            let column = prefix.rsplit('\n').next().unwrap_or_default().chars().count() + 1;
            return Err(self.sources.failure(
                ErrorKind::Startup,
                format!("{specifier}:{line}:{column}: {error}"),
                specifier.to_file_path().ok().and_then(|path| {
                    Some(SourceLocation {
                        path,
                        line: u32::try_from(line).ok()?,
                        column: u32::try_from(column).ok()?,
                    })
                }),
            ));
        }
        let is_script = !parsed.program.body.iter().any(Statement::is_module_declaration);
        let is_cjs = self
            .tracker
            .is_cjs_with_known_is_script(specifier, MediaType::from_specifier(specifier), is_script)
            .map_err(JsErrorBox::from_err)?;
        self.tracker.set_is_known_script(specifier, is_cjs);
        if !is_cjs {
            return Ok(CjsAnalysis::Esm(source, None));
        }
        let mut exports = Exports::default();
        exports.visit_program(&parsed.program);
        Ok(CjsAnalysis::Cjs(CjsAnalysisExports {
            exports: exports.names.into_iter().collect(),
            reexports: exports.reexports.into_iter().collect(),
            member_reexports: exports.member_reexports,
        }))
    }

    async fn analyze_cjs_member_props<'a>(
        &self,
        specifier: &Url,
        source: Option<Cow<'a, str>>,
        member: &str,
    ) -> Result<Option<Vec<String>>, JsErrorBox> {
        if is_json_or_addon(specifier) {
            return Ok(None);
        }
        let source =
            source.map(Ok).unwrap_or_else(|| read(specifier, &self.permissions).map(Cow::Owned))?;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::unambiguous()).parse();
        let mut exports = Exports::default();
        exports.visit_program(&parsed.program);
        let names = exports.members.get(member).or_else(|| {
            exports.aliases.get(member).and_then(|binding| exports.binding_properties.get(binding))
        });
        Ok(names.map(|names| names.iter().cloned().collect()))
    }
}

fn is_json_or_addon(specifier: &Url) -> bool {
    specifier.path().ends_with(".json") || specifier.path().ends_with(".node")
}

fn read(specifier: &Url, permissions: &PermissionsContainer) -> Result<String, JsErrorBox> {
    let path = specifier
        .to_file_path()
        .map_err(|_| JsErrorBox::type_error(format!("not a local module: {specifier}")))?;
    let checked = permissions
        .check_open(Cow::Borrowed(&path), OpenAccessKind::Read, Some("CommonJS export analysis"))
        .map_err(JsErrorBox::from_err)?;
    std::fs::read_to_string(checked).map_err(JsErrorBox::from_err)
}
