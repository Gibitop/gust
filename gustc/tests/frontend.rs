use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lexer::Lexer;
use gustc::parser::Parser;

#[test]
fn hello_world_has_no_frontend_errors() {
    let source = include_str!("../../examples/hello-world.gust");
    let result = check_source(source);

    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basics_parses_without_syntax_errors() {
    let source = include_str!("../../examples/milestone.gust");
    let (tokens, lexer_diagnostics) = Lexer::new(source).tokenize();
    let (_, parser_diagnostics) = Parser::new(tokens).parse();

    assert!(
        lexer_diagnostics.is_empty(),
        "expected no lexer diagnostics, got {lexer_diagnostics:?}"
    );
    assert!(
        parser_diagnostics.is_empty(),
        "expected no parser diagnostics, got {parser_diagnostics:?}"
    );
}

#[test]
fn basics_reports_unsupported_features() {
    let source = include_str!("../../examples/milestone.gust");
    let result = check_source(source);

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Warning
                && diagnostic.message.contains("not implemented yet")),
        "expected at least one unsupported-feature warning, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basic_primitive_type_names_are_valid() {
    let result = check_source(
        r#"
fn main() {
    let string: String
    let boolean: bool
    let unsigned8: u8
    let unsigned16: u16
    let unsigned32: u32
    let unsigned64: u64
    let unsigned128: u128
    let pointerSized: usize
    let signed8: i8
    let signed16: i16
    let signed32: i32
    let signed64: i64
    let signed128: i128
    let float32: f32
    let float64: f64
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected basic primitive types to be valid, got {:?}",
        result.diagnostics
    );
}

#[test]
fn floating_point_literals_and_arithmetic_validate() {
    let result = check_source(
        r#"
fn main() {
    let single: f32 = 1.25
    let singleSum = single + 1.25
    let reverseSingleSum = 1.25 + single
    let double = 6.02e23
    let mixed = 1 + 2.5
    let remainder: f64 = 5.5 % 2
    let ordered = mixed < 4.0
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected floating-point expressions to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn floating_point_literals_do_not_initialize_integer_types() {
    let result = check_source(
        r#"
fn main() {
    let count: i128 = 1.5
}
"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected value of type `i128`, got `f64`")),
        "expected integer initializer mismatch, got {:?}",
        result.diagnostics
    );
}

#[test]
fn bool_literals_validate_as_bool() {
    let result = check_source(
        r#"
fn main() {
    let enabled = true
    let disabled: bool = false
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected bool literals to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn if_else_statements_validate() {
    let result = check_source(
        r#"
fn main() {
    let enabled = true

    if enabled {
        io.println("enabled")
    } else if false {
        io.println("unreachable")
    } else {
        io.println("disabled")
    }
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected if/else statements to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn if_condition_must_be_bool() {
    let result = check_source(
        r#"
fn main() {
    if "not a bool" {}
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `bool`, got `String`")),
        "expected bool condition error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn if_branch_bindings_do_not_escape() {
    let result = check_source(
        r#"
fn main() {
    if true {
        let message = "scoped"
    }

    io.println(message)
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `message`")),
        "expected branch binding scope error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn returning_if_else_satisfies_explicit_return_type() {
    let result = check_source(
        r#"
fn choose(enabled: bool): String {
    if enabled {
        return "enabled"
    } else {
        return "disabled"
    }
}

fn main() {}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected returning if/else to satisfy return type, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basic_struct_literal_validates() {
    let result = check_source(
        r#"
struct Lang {
    name: String
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: 1,
    }
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected basic struct literal to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_field_access_validates_as_field_type() {
    let result = check_source(
        r#"
struct Lang {
    name: String
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: 1,
    }
    let name: String = lang.name
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected struct field access to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_missing_field_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: String
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
    }
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("missing field `version` in struct literal `Lang`")),
        "expected missing-field error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_unknown_field_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: String
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: 1,
    }
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("unknown field `version` for struct `Lang`")),
        "expected unknown-field error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_duplicate_field_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: String
}

fn main() {
    let lang = Lang {
        name: "Gust",
        name: "Gust",
    }
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("duplicate field `name` in struct literal")),
        "expected duplicate-field error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_field_type_mismatch_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: String
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: "1",
    }
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `u32`, got `String`")),
        "expected field type mismatch, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_methods_remain_unsupported() {
    let result = check_source(
        r#"
struct Lang {
    name: String

    fn displayName(): String {
        return self.name
    }
}

fn main() {}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Warning
                && diagnostic
                    .message
                    .contains("methods are parsed but method dispatch is not implemented yet")),
        "expected unsupported-method warning, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_type_names_are_errors() {
    let result = check_source(
        r#"
fn main() {
    let value: Nope = 1
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown type `Nope`")),
        "expected unknown-type error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_type_suppresses_followup_initializer_mismatch() {
    let result = check_source(
        r#"
fn main() {
    let value: Nope = "not checked again"
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown type `Nope`")),
        "expected unknown-type error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("expected value of type")),
        "expected Unknown to suppress mismatch cascades, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_identifier_suppresses_followup_initializer_mismatch() {
    let result = check_source(
        r#"
fn main() {
    let value: u32 = missing
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `missing`")),
        "expected unknown-name error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("expected value of type")),
        "expected Unknown to suppress mismatch cascades, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_type_suppresses_followup_default_error() {
    let result = check_source(
        r#"
fn main() {
    let value: Nope
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown type `Nope`")),
        "expected unknown-type error, got {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("default values are only supported")),
        "expected Unknown to suppress default cascades, got {:?}",
        result.diagnostics
    );
}

#[test]
fn let_without_type_or_value_is_an_error() {
    let result = check_source(
        r#"
fn main() {
    let value
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected `=` or type annotation")),
        "expected missing-let-value error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mismatched_annotated_initializer_is_an_error() {
    let result = check_source(
        r#"
fn main() {
    let value: u32 = "not a number"
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `u32`, got `String`")),
        "expected type-mismatch error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn string_add_validates_as_string() {
    let result = check_source(
        r#"
fn main() {
    let message: String = "Hello, " + "Gust"
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected string concat to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn nested_string_add_validates_as_string() {
    let result = check_source(
        r#"
fn main() {
    let name = "Gust"
    let message: String = "Hello, " + name + "!"
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected nested string concat to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn numeric_math_operators_validate() {
    let result = check_source(
        r#"
fn main() {
    let add = 1 + 2
    let subtract = 5 - 3
    let multiply = 4 * 2
    let divide = 8 / 2
    let remainder = 9 % 4
    let negative = -5
    let contextual: u64 = (1 + 2) * 3
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected numeric math operators to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn math_operators_require_numeric_operands() {
    let result = check_source(
        r#"
fn main() {
    let invalid = "Gust" * "Gust"
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("operator * only supports numeric operands")),
        "expected numeric operand diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unary_negation_requires_signed_numeric_operand() {
    let result = check_source(
        r#"
fn main() {
    let count: u32 = 5
    let invalid = -count
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("operator - only supports signed numeric operands")),
        "expected signed numeric operand diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_math_operand_suppresses_followup_operand_error() {
    let result = check_source(
        r#"
fn main() {
    let count = missing * 2
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `missing`")),
        "expected unknown-name error, got {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("operator * only supports numeric operands")),
        "expected Unknown to suppress arithmetic operand cascades, got {:?}",
        result.diagnostics
    );
}

#[test]
fn function_call_with_basic_return_type_validates() {
    let result = check_source(
        r#"
fn greet(name: String): String {
    return "Hello, " + name
}

fn main() {
    let message: String = greet("Gust")
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected function call to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn function_call_wrong_argument_count_is_an_error() {
    let result = check_source(
        r#"
fn greet(name: String): String {
    return name
}

fn main() {
    let message = greet()
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("function `greet` expects 1 arguments, got 0")),
        "expected wrong-argument-count error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn function_call_wrong_argument_type_is_an_error() {
    let result = check_source(
        r#"
fn greet(name: String): String {
    return name
}

fn main() {
    let message = greet(1)
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `String`, got `i32`")),
        "expected wrong-argument-type error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn function_return_type_mismatch_is_an_error() {
    let result = check_source(
        r#"
fn greet(): String {
    return 1
}

fn main() {}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `String`, got `i32`")),
        "expected return-type mismatch error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_call_callee_suppresses_initializer_mismatch() {
    let result = check_source(
        r#"
fn main() {
    let message: String = missing("Gust")
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `missing`")),
        "expected unknown-name error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("expected value of type")),
        "expected Unknown to suppress mismatch cascades, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_call_argument_suppresses_argument_mismatch() {
    let result = check_source(
        r#"
fn greet(name: String): String {
    return name
}

fn main() {
    let message = greet(missing)
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `missing`")),
        "expected unknown-name error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("expected value of type")),
        "expected Unknown to suppress argument mismatch cascades, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unannotated_numeric_literals_default_to_i32() {
    let result = check_source(
        r#"
fn main() {
    let value = 1
    let copy: u32 = value
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `u32`, got `i32`")),
        "expected default-i32 mismatch, got {:?}",
        result.diagnostics
    );
}

#[test]
fn duplicate_top_level_names_are_errors() {
    let result = check_source(
        r#"
fn main() {}
fn main() {}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("duplicate top-level name `main`")),
        "expected duplicate-name error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn missing_main_is_an_error() {
    let result = check_source("fn helper() {}");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("missing `main` function")),
        "expected missing-main error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_local_reference_is_an_error() {
    let result = check_source(
        r#"
fn main() {
    io.println(name)
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `name`")),
        "expected unknown-name error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn immutable_increment_is_an_error() {
    let result = check_source(
        r#"
fn main() {
    let age = 30
    age++
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("cannot mutate immutable binding `age`")),
        "expected immutable-mutation error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_locals_can_be_assigned_and_incremented() {
    let result = check_source(
        r#"
fn main() {
    let mut count: u32 = 1
    count = count + 2
    count++
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected mutable local operations to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn arithmetic_compound_assignments_validate() {
    let result = check_source(
        r#"
fn main() {
    let mut value: f64 = 10
    value += 5
    value -= 2
    value *= 3
    value /= 2
    value %= 4

    let mut message = "hello"
    message += " world"
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected compound assignments to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn bitwise_and_shift_operators_validate_for_integer_types() {
    let result = check_source(
        r#"
fn main() {
    let unsigned: u32 = 12
    let signed: i16 = 3
    let combined: u32 = unsigned & 10 | 1 ^ 4
    let shifted: i16 = signed << 2 >> 1

    let mut flags: u8 = 1
    flags &= 7
    flags |= 2
    flags ^= 1
    flags <<= 2
    flags >>= 1
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected integer bitwise operations to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn bitwise_and_shift_operators_reject_non_integer_types() {
    let result = check_source(
        r#"
fn main() {
    let floatValue = 1.5 & 1.5
    let boolValue = true << false
}
"#,
    );

    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic
                .message
                .contains("only supports integer operands"))
            .count(),
        2,
        "expected integer-only operator errors, got {:?}",
        result.diagnostics
    );
}

#[test]
fn bitwise_operator_precedence_matches_rust() {
    use gustc::ast::{BinaryOp, ExprKind, FunctionBody, Item, StmtKind};

    let result = check_source(
        r#"
fn main() {
    let value = 1 | 2 ^ 3 & 4 << 1 + 1
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected bitwise expression to validate, got {:?}",
        result.diagnostics
    );

    let Item::Function(function) = &result.program.items[0] else {
        panic!("expected function");
    };
    let FunctionBody::Block(body) = &function.body else {
        panic!("expected block body");
    };
    let StmtKind::Let {
        value: Some(value), ..
    } = &body.statements[0].kind
    else {
        panic!("expected initialized local");
    };
    let ExprKind::Binary {
        op: BinaryOp::BitwiseOr,
        right,
        ..
    } = &value.kind
    else {
        panic!("expected bitwise or at the expression root");
    };
    let ExprKind::Binary {
        op: BinaryOp::BitwiseXor,
        right,
        ..
    } = &right.kind
    else {
        panic!("expected bitwise xor below bitwise or");
    };
    let ExprKind::Binary {
        op: BinaryOp::BitwiseAnd,
        right,
        ..
    } = &right.kind
    else {
        panic!("expected bitwise and below bitwise xor");
    };
    let ExprKind::Binary {
        op: BinaryOp::ShiftLeft,
        right,
        ..
    } = &right.kind
    else {
        panic!("expected shift below bitwise and");
    };
    assert!(matches!(
        right.kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
}

#[test]
fn shift_tokens_do_not_break_nested_generic_types() {
    let source = r#"
fn main() {
    let values: ArrayList<ArrayList<i32>>=[]
}
"#;
    let (tokens, lexer_diagnostics) = Lexer::new(source).tokenize();
    let (_, parser_diagnostics) = Parser::new(tokens).parse();

    assert!(
        lexer_diagnostics.is_empty(),
        "expected no lexer diagnostics, got {lexer_diagnostics:?}"
    );
    assert!(
        parser_diagnostics.is_empty(),
        "expected no parser diagnostics, got {parser_diagnostics:?}"
    );
}

#[test]
fn compound_assignment_requires_a_mutable_binding() {
    let result = check_source(
        r#"
fn main() {
    let count = 1
    count += 2
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("cannot assign to immutable binding `count`")),
        "expected immutable-assignment error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn compound_assignment_uses_arithmetic_type_rules() {
    let result = check_source(
        r#"
fn main() {
    let mut enabled = true
    enabled += false
}
"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("operator + only supports numeric or String operands")),
        "expected compound-assignment operator error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn assignment_requires_a_mutable_binding() {
    let result = check_source(
        r#"
fn main() {
    let count = 1
    count = 2
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("cannot assign to immutable binding `count`")),
        "expected immutable-assignment error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn assignment_value_must_match_the_binding_type() {
    let result = check_source(
        r#"
fn main() {
    let mut message = "Gust"
    message = 1
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `String`, got `i32`")),
        "expected assignment-type error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn increment_requires_a_numeric_binding() {
    let result = check_source(
        r#"
fn main() {
    let mut message = "Gust"
    message++
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("operator ++ only supports numeric operands")),
        "expected numeric-increment error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comparison_operators_validate() {
    let result = check_source(
        r#"
fn main() {
    let age: u32 = 30
    let name = "Gust"

    let equal: bool = age == 30
    let notEqual: bool = age != 0
    let less: bool = age < 31
    let lessEqual: bool = 29 <= age
    let greater: bool = age > 29
    let greaterEqual: bool = age >= 30
    let sameName: bool = name == "Gust"
    let differentName: bool = name != "Rust"
    let sameFlag: bool = true == true
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected comparisons to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comparison_operands_must_have_matching_types() {
    let result = check_source(
        r#"
fn main() {
    let left: u32 = 1
    let right: i32 = 1
    let equal = left == right
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `u32`, got `i32`")),
        "expected comparison type mismatch, got {:?}",
        result.diagnostics
    );
}

#[test]
fn ordering_requires_numeric_operands() {
    let result = check_source(
        r#"
fn main() {
    let ordered = "Gust" < "Rust"
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("operator < only supports numeric operands")),
        "expected numeric ordering error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_equality_is_rejected_until_trait_equality_exists() {
    let result = check_source(
        r#"
struct Person {
    name: String
}

fn main() {
    let left = Person { name: "Gust" }
    let right = Person { name: "Gust" }
    let equal = left == right
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("operator == only supports numeric, bool, and String operands")),
        "expected unsupported struct equality error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn logical_operators_validate_as_bool() {
    let result = check_source(
        r#"
fn main() {
    let age: u32 = 30
    let enabled = true
    let disabled = false
    let allowed: bool = age >= 18 && enabled && !disabled
    let fallback: bool = disabled || allowed
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected logical operators to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn logical_operators_require_bool_operands() {
    let result = check_source(
        r#"
fn main() {
    let invalidAnd = "Gust" && true
    let invalidNot = !1
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `bool`, got `String`")),
        "expected logical operand error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `bool`, got `i32`")),
        "expected unary-not operand error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn logical_and_binds_more_tightly_than_logical_or() {
    use gustc::ast::{BinaryOp, ExprKind, FunctionBody, Item, StmtKind};

    let (tokens, lexer_diagnostics) =
        Lexer::new("fn main() { let value = true || false && false }").tokenize();
    let (program, parser_diagnostics) = Parser::new(tokens).parse();

    assert!(lexer_diagnostics.is_empty());
    assert!(parser_diagnostics.is_empty());

    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let FunctionBody::Block(block) = &function.body else {
        panic!("expected block body");
    };
    let StmtKind::Let {
        value: Some(value), ..
    } = &block.statements[0].kind
    else {
        panic!("expected initialized local");
    };
    let ExprKind::Binary {
        op: BinaryOp::LogicalOr,
        right,
        ..
    } = &value.kind
    else {
        panic!("expected logical or at the expression root");
    };

    assert!(matches!(
        right.kind,
        ExprKind::Binary {
            op: BinaryOp::LogicalAnd,
            ..
        }
    ));
}

#[test]
fn payload_enums_and_exhaustive_matches_validate() {
    let result = check_source(
        r#"struct Person {
    name: String
}

enum Being {
    Person(Person)
    Unknown
}

fn greeting(being: Being): String {
    return match being {
        Being.Person(person) => "Hello, " + person.name,
        Being.Unknown => "Hello, stranger",
    }
}

fn main() {
    let being = Being.Person(Person { name: "Ada" })
    io.println(greeting(being))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected enums and matches to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn enum_matches_must_be_exhaustive() {
    let result = check_source(
        r#"enum Status {
    Ready
    Waiting
}

fn label(status: Status): String {
    return match status {
        Status.Ready => "ready",
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("non-exhaustive match for enum `Status`; missing `Waiting`")
        }),
        "expected non-exhaustive match error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn enum_payloads_and_match_branches_are_type_checked() {
    let result = check_source(
        r#"enum Result {
    Value(String)
    Empty
}

fn label(result: Result): String {
    return match result {
        Result.Value(value) => value,
        Result.Empty => false,
    }
}

fn main() {
    let invalid = Result.Value(false)
}"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.severity == Severity::Error
                    && diagnostic
                        .message
                        .contains("expected value of type `String`, got `bool`")
            })
            .count()
            >= 2,
        "expected payload and branch type errors, got {:?}",
        result.diagnostics
    );
}

#[test]
fn enum_match_patterns_reject_duplicate_and_invalid_bindings() {
    let result = check_source(
        r#"enum State {
    Named(String)
    Empty
}

fn label(state: State): String {
    return match state {
        State.Named(name) => name,
        State.Named(other) => other,
        State.Empty(value) => value,
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("duplicate match branch for variant `Named`")
        }),
        "expected duplicate branch error, got {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("unit variant `State.Empty` does not bind a payload")
        }),
        "expected invalid binding error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn payload_pattern_error_suggests_valid_syntax() {
    let result = check_source(
        r#"enum Being {
    Dog(String)
}

fn label(being: Being): String {
    return match being {
        Being.Dog => "dog",
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains(
                    "`Being.Dog` contains a `String` value; use `Being.Dog(value)` to bind it or `Being.Dog(_)` to ignore it",
                )
        }),
        "expected actionable payload-pattern error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn underscore_discards_an_enum_payload_without_creating_a_binding() {
    let result = check_source(
        r#"enum Being {
    Dog(String)
}

fn label(being: Being): String {
    return match being {
        Being.Dog(_) => "dog",
    }
}

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected underscore payload discard to validate, got {:?}",
        result.diagnostics
    );

    let result = check_source(
        r#"enum Being {
    Dog(String)
}

fn label(being: Being): String {
    return match being {
        Being.Dog(_) => _,
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `_`")
        }),
        "expected underscore not to create a binding, got {:?}",
        result.diagnostics
    );
}

#[test]
fn enum_variants_are_namespaced_by_their_enum() {
    let result = check_source(
        r#"enum Left {
    Value(String)
}

enum Right {
    Value(String)
}

fn leftLabel(value: Left): String {
    return match value {
        Left.Value(label) => label,
    }
}

fn rightLabel(value: Right): String {
    return match value {
        Right.Value(label) => label,
    }
}

fn main() {
    io.println(leftLabel(Left.Value("left")))
    io.println(rightLabel(Right.Value("right")))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected enums to reuse qualified variant names, got {:?}",
        result.diagnostics
    );
}

#[test]
fn enum_variants_cannot_be_used_without_the_enum_name() {
    let result = check_source(
        r#"enum Status {
    Ready
}

fn main() {
    let status = Ready
}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `Ready`")
        }),
        "expected unqualified variant error, got {:?}",
        result.diagnostics
    );

    let result = check_source(
        r#"enum Status {
    Ready
}

fn label(status: Status): String {
    return match status {
        Ready => "ready",
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.message.contains("expected `.`")
        }),
        "expected qualified pattern syntax error, got {:?}",
        result.diagnostics
    );
}
