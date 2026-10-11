use std::path::Path;

use oxc::allocator::Allocator;
use oxc::ast::ast::{
    ExportAllDeclaration, ExportFromDeclaration, Expression, ImportDeclaration, ImportExpression,
    StringLiteral,
};
use oxc::ast_visit::Visit;
use oxc::parser::Parser;
use oxc::span::SourceType;

pub(crate) fn import_position(path: &Path, source: &str, specifier: &str) -> Option<(u32, u32)> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).ok()?;
    let parsed = Parser::new(&allocator, source, source_type).parse();
    let mut imports = ImportPosition { specifier, offset: None };
    imports.visit_program(&parsed.program);
    let prefix = &source[..imports.offset?];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix.rsplit('\n').next()?.encode_utf16().count() as u32 + 1;
    Some((line, column))
}

struct ImportPosition<'a> {
    specifier: &'a str,
    offset: Option<usize>,
}

impl ImportPosition<'_> {
    fn record(&mut self, source: &StringLiteral<'_>) {
        if self.offset.is_none() && source.value == self.specifier {
            self.offset = Some(source.span.start as usize);
        }
    }
}

impl<'a> Visit<'a> for ImportPosition<'_> {
    fn visit_import_declaration(&mut self, import: &ImportDeclaration<'a>) {
        self.record(&import.source);
    }

    fn visit_export_from_declaration(&mut self, export: &ExportFromDeclaration<'a>) {
        self.record(&export.source);
    }

    fn visit_export_all_declaration(&mut self, export: &ExportAllDeclaration<'a>) {
        self.record(&export.source);
    }

    fn visit_import_expression(&mut self, import: &ImportExpression<'a>) {
        if let Expression::StringLiteral(source) = &import.source {
            self.record(source);
        }
    }
}
