use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{CallExpression, Expression};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::project::classify::FileKind;

pub fn classify_by_content(abs_path: &Path) -> FileKind {
    let Ok(source_text) = std::fs::read_to_string(abs_path) else {
        return FileKind::Unknown;
    };
    let source_type = SourceType::from_path(abs_path).unwrap_or_default();
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &source_text, source_type).parse();

    let program = ret.program;
    let mut detector = TestCallDetector::default();
    detector.visit_program(&program);
    if detector.found_test_call {
        FileKind::Test
    } else {
        FileKind::Production
    }
}

#[derive(Debug, Default)]
struct TestCallDetector {
    found_test_call: bool,
}

impl TestCallDetector {
    fn callee_base_ident<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
        match callee {
            Expression::Identifier(ident) => Some(ident.name.as_str()),
            Expression::StaticMemberExpression(member) => match &member.object {
                Expression::Identifier(ident) => Some(ident.name.as_str()),
                _ => None,
            },
            Expression::ComputedMemberExpression(member) => match &member.object {
                Expression::Identifier(ident) => Some(ident.name.as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    fn is_test_fn_name(name: &str) -> bool {
        matches!(name, "describe" | "it" | "test")
    }
}

impl<'a> Visit<'a> for TestCallDetector {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.found_test_call {
            return;
        }
        if let Some(name) = Self::callee_base_ident(&it.callee)
            && Self::is_test_fn_name(name)
        {
            self.found_test_call = true;
            return;
        }
        walk::walk_call_expression(self, it);
    }
}
