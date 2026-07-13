use gustc::ast::Item;
use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lexer::Lexer;
use gustc::parser::Parser;

include!("smoke.rs");
include!("types_control.rs");
include!("structs_methods.rs");
include!("calls_mutation_ops.rs");
include!("calls_mutation_more.rs");
include!("matches.rs");
include!("generics_traits.rs");
include!("generics_traits_more.rs");
include!("associated_types.rs");
include!("indexing.rs");
