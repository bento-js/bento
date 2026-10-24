use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use deno_core::url::Url;
use deno_error::JsErrorBox;
use deno_media_type::MediaType;
use deno_runtime::deno_permissions::{OpenAccessKind, PermissionsContainer};
use node_resolver::analyze::{CjsAnalysis, CjsAnalysisExports, CjsMemberReExport, EsmAnalysisMode};
use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast_visit::{Visit, walk};
use oxc::parser::Parser;
use oxc::span::SourceType;

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

#[derive(Default)]
struct Exports {
    names: BTreeSet<String>,
    reexports: BTreeSet<String>,
    members: BTreeMap<String, BTreeSet<String>>,
    aliases: BTreeMap<String, String>,
    binding_properties: BTreeMap<String, BTreeSet<String>>,
    member_reexports: Vec<CjsMemberReExport>,
}

fn identifier(expression: &Expression<'_>, name: &str) -> bool {
    matches!(expression, Expression::Identifier(identifier) if identifier.name == name)
}

fn module_exports(expression: &Expression<'_>) -> bool {
    expression.as_member_expression().is_some_and(|member| {
        member.static_property_name() == Some("exports") && identifier(member.object(), "module")
    })
}

fn exports_object(expression: &Expression<'_>) -> bool {
    identifier(expression, "exports") || module_exports(expression)
}

fn required<'a>(expression: &'a Expression<'_>) -> Option<&'a str> {
    let Expression::CallExpression(call) = expression else {
        return None;
    };
    if !identifier(&call.callee, "require") {
        return None;
    }
    match call.arguments.first()? {
        Argument::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

fn object_names<'a>(expression: &'a Expression<'_>) -> impl Iterator<Item = String> + 'a {
    let object = match expression {
        Expression::ObjectExpression(object) => Some(object),
        _ => None,
    };
    object.into_iter().flat_map(|object| &object.properties).filter_map(|property| match property {
        ObjectPropertyKind::ObjectProperty(property) => {
            property.key.static_name().map(|name| name.into_owned())
        }
        _ => None,
    })
}

impl Exports {
    fn expression_exports(&mut self, expression: &Expression<'_>) {
        self.names.extend(object_names(expression));
        if let Some(specifier) = required(expression) {
            self.reexports.insert(specifier.to_owned());
        }
    }

    fn assignment(&mut self, member: &MemberExpression<'_>, right: &Expression<'_>) {
        if exports_object(member.object()) {
            let Some(name) = member.static_property_name() else {
                return;
            };
            self.names.insert(name.to_owned());
            let names = object_names(right).collect::<BTreeSet<_>>();
            if !names.is_empty() {
                self.members.insert(name.to_owned(), names);
            }
            if let Expression::Identifier(binding) = right {
                self.aliases.insert(name.to_owned(), binding.name.to_string());
            }
            return;
        }
        if member.static_property_name() == Some("exports") && identifier(member.object(), "module")
        {
            self.names.extend(object_names(right));
            if let Some(specifier) = required(right) {
                self.reexports.insert(specifier.to_owned());
            }
            if let Some(inner) = right.as_member_expression()
                && let (Some(specifier), Some(member)) =
                    (required(inner.object()), inner.static_property_name())
            {
                self.member_reexports.push(CjsMemberReExport {
                    specifier: specifier.to_owned(),
                    member: member.to_owned(),
                });
            }
            return;
        }
        if let (Expression::Identifier(binding), Some(name)) =
            (member.object(), member.static_property_name())
        {
            self.binding_properties
                .entry(binding.name.to_string())
                .or_default()
                .insert(name.to_owned());
        }
    }
}

impl<'a> Visit<'a> for Exports {
    fn visit_assignment_expression(&mut self, assignment: &AssignmentExpression<'a>) {
        if let Some(member) = assignment.left.as_member_expression() {
            self.assignment(member, &assignment.right);
        }
        walk::walk_assignment_expression(self, assignment);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        let method = call.callee.as_member_expression();
        if let Some(method) = method
            && identifier(method.object(), "Object")
            && matches!(
                method.static_property_name(),
                Some("defineProperty" | "defineProperties" | "assign")
            )
            && let Some(target) = call.arguments.first().and_then(Argument::as_expression)
            && exports_object(target)
        {
            match call.arguments.get(1) {
                Some(Argument::StringLiteral(name))
                    if method.static_property_name() == Some("defineProperty") =>
                {
                    self.names.insert(name.value.to_string());
                }
                Some(argument) => {
                    if let Some(expression) = argument.as_expression() {
                        self.expression_exports(expression);
                    }
                }
                None => {}
            }
        }
        if let Expression::Identifier(name) = &call.callee
            && matches!(
                name.name.as_str(),
                "__export" | "__exportStar" | "_export" | "_export_star"
            )
        {
            for expression in call.arguments.iter().filter_map(Argument::as_expression) {
                self.expression_exports(expression);
            }
        }
        walk::walk_call_expression(self, call);
    }
}
