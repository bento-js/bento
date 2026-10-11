use oxc::ast::ast::{Decorator, JSXElement, JSXFragment, Program};
use oxc::ast_visit::{Visit, walk};
use oxc::span::Span;

/// Where a parsed file first uses syntax that only executes under a specific setting.
#[derive(Debug, Default)]
pub(crate) struct SyntaxSurvey {
    pub first_jsx: Option<Span>,
    pub first_decorator: Option<Span>,
}

impl SyntaxSurvey {
    pub fn of(program: &Program<'_>) -> Self {
        let mut survey = Self::default();
        survey.visit_program(program);
        survey
    }
}

impl<'a> Visit<'a> for SyntaxSurvey {
    fn visit_jsx_element(&mut self, element: &JSXElement<'a>) {
        self.first_jsx.get_or_insert(element.span);
        walk::walk_jsx_element(self, element);
    }

    fn visit_jsx_fragment(&mut self, fragment: &JSXFragment<'a>) {
        self.first_jsx.get_or_insert(fragment.span);
        walk::walk_jsx_fragment(self, fragment);
    }

    fn visit_decorator(&mut self, decorator: &Decorator<'a>) {
        self.first_decorator.get_or_insert(decorator.span);
        walk::walk_decorator(self, decorator);
    }
}
