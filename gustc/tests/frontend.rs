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
    let pointerSized: usize
    let signed8: i8
    let signed16: i16
    let signed32: i32
    let signed64: i64
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
fn numeric_add_is_rejected_for_now() {
    let result = check_source(
        r#"
fn main() {
    let count = 1 + 2
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
                    .contains("operator + only supports String operands for now")),
        "expected string-only add diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_add_operand_suppresses_followup_string_operand_error() {
    let result = check_source(
        r#"
fn main() {
    let message = missing + "!"
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
            .contains("operator + only supports String operands")),
        "expected Unknown to suppress concat operand cascades, got {:?}",
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
