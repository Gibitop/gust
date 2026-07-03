use gustc::c_codegen::emit_c;
use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lower::{LoweredStatement, LoweredValue, lower_program};

#[test]
fn hello_world_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    io.println("Hello, world!")
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("hello world should lower");

    assert_eq!(
        lowered.statements,
        vec![LoweredStatement::Println(LoweredValue::StringLiteral(
            "Hello, world!".to_string()
        ))]
    );
}

#[test]
fn string_local_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    let message = "Hello, string local!"
    io.println(message)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("string local should lower");

    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::StringLocal {
                name: "message".to_string(),
                value: "Hello, string local!".to_string(),
            },
            LoweredStatement::Println(LoweredValue::StringLocal("message".to_string())),
        ]
    );
}

#[test]
fn hello_world_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    io.println("Hello, world!")
}"#,
    );
    let lowered = lower_program(&result.program).expect("hello world should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n\nint main(void) {\n    puts(\"Hello, world!\");\n    return 0;\n}\n"
    );
}

#[test]
fn string_local_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let message = "Hello, string local!"
    io.println(message)
}"#,
    );
    let lowered = lower_program(&result.program).expect("string local should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n\nint main(void) {\n    const char* message = \"Hello, string local!\";\n    puts(message);\n    return 0;\n}\n"
    );
}

#[test]
fn c_output_escapes_string_values() {
    let result = check_source(
        r#"fn main() {
    io.println("line\n\"quote\"\\slash")
}"#,
    );
    let lowered = lower_program(&result.program).expect("escaped string should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n\nint main(void) {\n    puts(\"line\\n\\\"quote\\\"\\\\slash\");\n    return 0;\n}\n"
    );
}

#[test]
fn unsupported_string_local_forms_are_errors() {
    let result = check_source(
        r#"fn main() {
    let count = 1
    io.println(count)
    let mut mutable = "Gust"
    io.println(mutable)
    let typed: String = "Gust"
    io.println(typed)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let diagnostics = lower_program(&result.program).expect_err("source should not lower");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("only string literal let values are supported")),
        "expected non-string local diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown string local `count`")),
        "expected unknown count diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("`let mut` bindings are not supported")),
        "expected mutable local diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("typed let bindings are not supported")),
        "expected typed local diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn unknown_println_local_is_frontend_error() {
    let result = check_source(
        r#"fn main() {
    io.println(message)
}"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `message`")),
        "expected frontend unknown-name diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basics_reaches_build_mode_rejection() {
    let source = include_str!("../../examples/basics.gust");
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected basics frontend to avoid hard errors, got {:?}",
        result.diagnostics
    );

    let diagnostics = lower_program(&result.program).expect_err("basics should not lower");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("imports are not supported in executable builds")),
        "expected unsupported-import diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("structs are not supported in executable builds")),
        "expected unsupported-struct diagnostic, got {diagnostics:?}"
    );
}
