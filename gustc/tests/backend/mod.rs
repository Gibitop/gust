use gustc::ast::BasicType;
use gustc::c_codegen::emit_c;
use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lower::{
    LoweredExpr, LoweredExprKind, LoweredField, LoweredFunction, LoweredParam, LoweredStatement,
    LoweredStruct, LoweredStructFieldValue, LoweredType, lower_program,
};

fn basic(type_: BasicType) -> LoweredType {
    LoweredType::Basic(type_)
}

include!("lowering.rs");
include!("lowering_more.rs");
include!("c_output.rs");
include!("mutation_members.rs");
include!("operators.rs");
include!("matches.rs");
include!("generics_traits.rs");
