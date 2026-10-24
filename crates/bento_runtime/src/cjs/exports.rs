//! The hand-written Oxc visitor recognizing the CommonJS export patterns bundlers emit.

use std::collections::{BTreeMap, BTreeSet};

use node_resolver::analyze::CjsMemberReExport;
use oxc::ast::ast::{
    Argument, AssignmentExpression, CallExpression, Expression, MemberExpression,
    ObjectPropertyKind,
};
use oxc::ast_visit::{Visit, walk};

#[derive(Default)]
pub(super) struct Exports {
    pub(super) names: BTreeSet<String>,
    pub(super) reexports: BTreeSet<String>,
    pub(super) members: BTreeMap<String, BTreeSet<String>>,
    pub(super) aliases: BTreeMap<String, String>,
    pub(super) binding_properties: BTreeMap<String, BTreeSet<String>>,
    pub(super) member_reexports: Vec<CjsMemberReExport>,
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
