use gustc::c_codegen::emit_c;
use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lower::{LoweredStatement, lower_program};

#[test]
fn hello_world_lowers_successfully() {
    let source = include_str!("../../examples/hello-world.gust");
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("hello world should lower");

    assert_eq!(
        lowered.statements,
        vec![LoweredStatement::Println("Hello, world!".to_string())]
    );
}

#[test]
fn hello_world_c_output_is_stable() {
    let source = include_str!("../../examples/hello-world.gust");
    let result = check_source(source);
    let lowered = lower_program(&result.program).expect("hello world should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n\nint main(void) {\n    puts(\"Hello, world!\");\n    return 0;\n}\n"
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
fn unsupported_executable_features_are_errors() {
    let result = check_source(
        r#"fn main() {
    let name = "Gust"
    io.println(name)
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
                    .contains("let statements are not supported")),
        "expected unsupported-let diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("only accepts a string literal")),
        "expected non-string println diagnostic, got {diagnostics:?}"
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
