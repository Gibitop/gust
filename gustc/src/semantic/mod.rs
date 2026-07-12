use std::collections::{HashMap, HashSet};

use crate::ast::{
    BasicType, BinaryOp, Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, ImplDecl,
    Item, MatchBranchBody, Pattern, Program, Stmt, StmtKind, StructDecl, StructInitField,
    StructMember, TraitDecl, TraitMethodDecl, TypeRef, UnaryOp, number_literal_is_float,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

pub fn validate(program: &Program) -> Vec<Diagnostic> {
    let program = match crate::monomorphize::monomorphize(program) {
        Ok(program) => program,
        Err(diagnostics) => return diagnostics,
    };
    let mut analyzer = Analyzer::new();
    analyzer.collect_top_level(&program);
    analyzer.validate_program(&program);
    analyzer.diagnostics
}

include!("names.rs");
include!("state.rs");
include!("types.rs");
include!("collect.rs");
include!("program.rs");
include!("expressions.rs");
include!("calls.rs");
include!("operators.rs");
include!("patterns.rs");
include!("usefulness.rs");
include!("scope.rs");
