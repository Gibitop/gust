use gustc::ast::BasicType;
use gustc::c_codegen::{CCodegenOptions, emit_c, emit_c_with_options};
use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lower::{
    LoweredExpr, LoweredExprKind, LoweredField, LoweredFunction, LoweredParam,
    LoweredSourceLocation, LoweredStatement, LoweredStruct, LoweredStructFieldValue, LoweredType,
    lower_program, lower_program_with_source,
};
use std::{fs, process::Command};

fn basic(type_: BasicType) -> LoweredType {
    LoweredType::Basic(type_)
}

fn source_location(line: usize) -> LoweredSourceLocation {
    LoweredSourceLocation {
        path: "<source>".to_string(),
        line,
        column: 1,
    }
}

include!("lowering.rs");
include!("lowering_more.rs");
include!("c_output.rs");
include!("mutation_members.rs");
include!("operators.rs");
include!("matches.rs");
include!("generics_traits.rs");
