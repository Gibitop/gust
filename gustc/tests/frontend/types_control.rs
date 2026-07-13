#[test]
fn basic_primitive_type_names_are_valid() {
    let result = check_source(
        r#"
fn main() {
    let string: string
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
fn numeric_as_casts_validate() {
    let result = check_source(
        r#"
fn main() {
    let integer = 42 as u8
    let wider = integer as i64
    let decimal = wider as f64
    let narrowed = decimal as i32
    let codePoint = 'A' as u32
    let letter = 65 as char
    let typedByte: u8 = 214
    let typedLetter = typedByte as char
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected numeric casts to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn as_casts_reject_unsupported_types() {
    let result = check_source(
        r#"
fn main() {
    let text = "42" as i32
    let invalid = 42 as string
    let intValue: i32 = 65
    let invalidChar = intValue as char
    let invalidFloatChar = 65.0 as char
    let invalidCharFloat = 'A' as f64
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("got `string` as `i32`")),
        "expected nonnumeric source cast error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("got `i32` as `string`")),
        "expected nonnumeric target cast error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("got `i32` as `char`")),
        "expected non-u8 char target cast error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("got `f64` as `char`")),
        "expected float-to-char cast error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("got `char` as `f64`")),
        "expected char-to-float cast error, got {:?}",
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
                    .contains("expected value of type `bool`, got `string`")),
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
fn while_statements_validate() {
    let result = check_source(
        r#"
fn main() {
    let mut index = 0

    while index < 5 {
        index += 1

        if index == 2 {
            continue
        }

        if index == 4 {
            break
        }
    }
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected while statement to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn for_loops_require_iterator_or_iterable_values() {
    let result = check_source(
        r#"fn main() {
    for value in 1 {
        io.println(value.toString())
    }
}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("`for` requires an `Iterator` or `Iterable`")
    }));
}

#[test]
fn while_condition_must_be_bool() {
    let result = check_source(
        r#"
fn main() {
    while "not a bool" {}
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
                    .contains("expected value of type `bool`, got `string`")),
        "expected bool condition error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn while_branch_bindings_do_not_escape() {
    let result = check_source(
        r#"
fn main() {
    while true {
        let message = "scoped"
        break
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
        "expected loop binding scope error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn break_and_continue_require_loop() {
    let result = check_source(
        r#"
fn main() {
    break
    continue
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
                    .contains("`break` can only be used inside a loop")),
        "expected break context error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("`continue` can only be used inside a loop")),
        "expected continue context error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn returning_if_else_satisfies_explicit_return_type() {
    let result = check_source(
        r#"
fn choose(enabled: bool): string {
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
