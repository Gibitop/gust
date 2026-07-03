use gustc::ast::BasicType;
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
            LoweredStatement::Local {
                name: "message".to_string(),
                type_: BasicType::String,
                value: LoweredValue::StringLiteral("Hello, string local!".to_string()),
            },
            LoweredStatement::Println(LoweredValue::Local("message".to_string())),
        ]
    );
}

#[test]
fn string_concat_local_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    let name = "Gust"
    let message = "Hello, " + name
    io.println(message)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("string concat local should lower");

    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::Local {
                name: "name".to_string(),
                type_: BasicType::String,
                value: LoweredValue::StringLiteral("Gust".to_string()),
            },
            LoweredStatement::Local {
                name: "message".to_string(),
                type_: BasicType::String,
                value: LoweredValue::StringConcat(
                    Box::new(LoweredValue::StringLiteral("Hello, ".to_string())),
                    Box::new(LoweredValue::Local("name".to_string())),
                ),
            },
            LoweredStatement::Println(LoweredValue::Local("message".to_string())),
        ]
    );
}

#[test]
fn direct_string_concat_println_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    let name = "Gust"
    io.println("Hello, " + name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("direct string concat should lower");

    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::Local {
                name: "name".to_string(),
                type_: BasicType::String,
                value: LoweredValue::StringLiteral("Gust".to_string()),
            },
            LoweredStatement::Println(LoweredValue::StringConcat(
                Box::new(LoweredValue::StringLiteral("Hello, ".to_string())),
                Box::new(LoweredValue::Local("name".to_string())),
            )),
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
        "#include <stdio.h>\n\nint main(void) {\n    const char* gust_message = \"Hello, string local!\";\n    puts(gust_message);\n    return 0;\n}\n"
    );
}

#[test]
fn string_concat_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let name = "Gust"
    let message = "Hello, " + name + "!"
    io.println("Inline " + "concat")
    io.println(message)
}"#,
    );
    let lowered = lower_program(&result.program).expect("string concat should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n\nstatic char* gust_concat(const char* left, const char* right) {\n    size_t left_len = strlen(left);\n    size_t right_len = strlen(right);\n    char* result = malloc(left_len + right_len + 1);\n    memcpy(result, left, left_len);\n    memcpy(result + left_len, right, right_len + 1);\n    return result;\n}\n\nint main(void) {\n    const char* gust_name = \"Gust\";\n    const char* gust_message = gust_concat(gust_concat(\"Hello, \", gust_name), \"!\");\n    puts(gust_concat(\"Inline \", \"concat\"));\n    puts(gust_message);\n    return 0;\n}\n"
    );
}

#[test]
fn basic_local_defaults_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let message: String
    let count: i32
    let flag: bool
    let byte: u8
    let size: usize
}"#,
    );
    let lowered = lower_program(&result.program).expect("basic defaults should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdbool.h>\n#include <stddef.h>\n#include <stdint.h>\n#include <stdio.h>\n\nint main(void) {\n    const char* gust_message = \"\";\n    int32_t gust_count = 0;\n    bool gust_flag = false;\n    uint8_t gust_byte = 0;\n    size_t gust_size = 0;\n    return 0;\n}\n"
    );
}

#[test]
fn initialized_basic_locals_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let message = "Hello, initialized!"
    let count: u64 = 42
    let flag = true
}"#,
    );
    let lowered = lower_program(&result.program).expect("initialized basics should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdbool.h>\n#include <stdint.h>\n#include <stdio.h>\n\nint main(void) {\n    const char* gust_message = \"Hello, initialized!\";\n    uint64_t gust_count = 42;\n    bool gust_flag = true;\n    return 0;\n}\n"
    );
}

#[test]
fn c_output_mangles_local_names_that_are_c_keywords() {
    let result = check_source(
        r#"fn main() {
    let short: u16 = 16
    let unsigned: u32 = 32
    let signed = 32
}"#,
    );
    let lowered = lower_program(&result.program).expect("keyword-like locals should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdint.h>\n#include <stdio.h>\n\nint main(void) {\n    uint16_t gust_short = 16;\n    uint32_t gust_unsigned = 32;\n    int32_t gust_signed = 32;\n    return 0;\n}\n"
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
fn println_rejects_non_string_operands() {
    let result = check_source(
        r#"fn main() {
    let count = 1
    io.println(count)
    let flag = true
    io.println(flag)
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
                && diagnostic.message.contains("only accepts `String` values")
                && diagnostic.message.contains("`i32`")),
        "expected numeric println diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("only accepts `String` values")
                && diagnostic.message.contains("`bool`")),
        "expected bool println diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn mutable_local_is_still_rejected_by_backend() {
    let result = check_source(
        r#"fn main() {
    let mut message = "Gust"
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
                    .contains("`let mut` bindings are not supported")),
        "expected mutable local diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn typed_non_basic_local_is_rejected_by_backend() {
    let result = check_source(
        r#"
struct Person {
    name: String
}

fn main() {
    let person: Person = Person {
        name: "Gust",
    }
}
"#,
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
                    .contains("only basic local types are supported")),
        "expected non-basic local diagnostic, got {diagnostics:?}"
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
    let source = include_str!("../../examples/milestone.gust");
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
