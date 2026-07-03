pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod span;

use ast::Program;
use diagnostic::{Diagnostic, Severity};
use lexer::Lexer;
use parser::Parser;
use semantic::validate;

pub struct CompileResult {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
}

impl CompileResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

pub fn check_source(source: &str) -> CompileResult {
    let (tokens, mut diagnostics) = Lexer::new(source).tokenize();
    let (program, parser_diagnostics) = Parser::new(tokens).parse();
    diagnostics.extend(parser_diagnostics);

    if !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        diagnostics.extend(validate(&program));
    }

    CompileResult {
        program,
        diagnostics,
    }
}
