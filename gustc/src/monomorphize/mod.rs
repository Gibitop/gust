use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    Block, ElseBranch, EnumDecl, Expr, ExprKind, FunctionBody, FunctionDecl, ImplDecl, Item,
    MatchBranchBody, Pattern, Program, Stmt, StmtKind, StructDecl, StructMember, TraitDecl,
    TypeParamBound, TypeRef,
};
use crate::diagnostic::Diagnostic;

pub fn monomorphize(program: &Program) -> Result<Program, Vec<Diagnostic>> {
    Monomorphizer::new(program).run(program)
}
include!("state.rs");
include!("types.rs");
include!("collect.rs");
include!("queue.rs");
include!("rewrite.rs");
include!("rewrite_expr.rs");
include!("inference.rs");
include!("return_inference.rs");
include!("type_arguments.rs");
include!("expr_inference.rs");
include!("specialize.rs");
include!("bounds.rs");
include!("impl_coherence.rs");
include!("reachability.rs");
