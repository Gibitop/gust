#[test]
fn numeric_comparisons_lower_with_contextual_literals() {
    let result = check_source(
        r#"fn main() {
    let age: u32 = 30

    if 18 <= age {
        io.println("adult")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("numeric comparison should lower");
    let LoweredStatement::If { condition, .. } = &lowered.statements[1] else {
        panic!("expected lowered if statement");
    };
    let LoweredExprKind::Comparison { left, op, right } = &condition.kind else {
        panic!("expected lowered comparison");
    };

    assert_eq!(*op, gustc::ast::BinaryOp::LessEqual);
    assert_eq!(left.type_, basic(BasicType::U32));
    assert_eq!(right.type_, basic(BasicType::U32));
}

#[test]
fn string_comparisons_emit_value_equality_runtime_helper() {
    let result = check_source(
        r#"fn main() {
    let name = "Gu" + "st"

    if name == "Gust" {
        io.println("equal")
    }

    if name != "Rust" {
        io.println("not equal")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("string comparisons should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("static bool gust_rt_string_equal("));
    assert!(source.contains("return left.gust_byte_len == right.gust_byte_len"));
    assert!(source.contains("memcmp(left.gust_data, right.gust_data, left.gust_byte_len) == 0;"));
    assert!(source.contains("if (gust_rt_string_equal(gust_name, (gust_rt_string){"));
    assert!(source.contains("if (!gust_rt_string_equal(gust_name, (gust_rt_string){"));
}

#[test]
fn numeric_comparisons_emit_native_c_operators() {
    let result = check_source(
        r#"fn main() {
    let age: u32 = 30
    let equal = age == 30
    let notEqual = age != 0
    let less = age < 31
    let lessEqual = age <= 30
    let greater = age > 29
    let greaterEqual = age >= 30
}"#,
    );
    let lowered = lower_program(&result.program).expect("numeric comparisons should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("(gust_age == 30)"));
    assert!(source.contains("(gust_age != 0)"));
    assert!(source.contains("(gust_age < 31)"));
    assert!(source.contains("(gust_age <= 30)"));
    assert!(source.contains("(gust_age > 29)"));
    assert!(source.contains("(gust_age >= 30)"));
}

#[test]
fn math_operators_lower_and_emit_native_c_with_precedence() {
    let result = check_source(
        r#"fn main() {
    let value: i32 = -2 + 3 * 4 - 8 / 2 % 3
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("math operators should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("(((-2) + (3 * 4)) - ((8 / 2) % 3))"));
}

#[test]
fn numeric_add_preserves_annotated_integer_type() {
    let result = check_source(
        r#"fn main() {
    let value: u64 = 1 + 2
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("numeric add should lower");

    assert_eq!(
        lowered.statements[0],
        LoweredStatement::Local {
            name: "value".to_string(),
            value: LoweredExpr {
                type_: basic(BasicType::U64),
                kind: LoweredExprKind::Arithmetic {
                    left: Box::new(LoweredExpr {
                        type_: basic(BasicType::U64),
                        kind: LoweredExprKind::NumberLiteral("1".to_string()),
                    }),
                    op: gustc::ast::BinaryOp::Add,
                    right: Box::new(LoweredExpr {
                        type_: basic(BasicType::U64),
                        kind: LoweredExprKind::NumberLiteral("2".to_string()),
                    }),
                },
            },
        }
    );
}

#[test]
fn numeric_as_casts_lower_and_emit_c_casts() {
    let result = check_source(
        r#"fn main() {
    let value = 300 as u8
    let widened = value as i64
    let decimal = widened as f64
    let saturated = 999.9 as u8
    let nan = (0.0 / 0.0) as i32
    let letter = 65 as char
    let codePoint = letter as u32
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("numeric casts should lower");
    let LoweredStatement::Local { value, .. } = &lowered.statements[0] else {
        panic!("expected lowered local");
    };
    let LoweredExprKind::Cast { value: inner, type_ } = &value.kind else {
        panic!("expected lowered cast");
    };

    assert_eq!(value.type_, basic(BasicType::U8));
    assert_eq!(*type_, basic(BasicType::U8));
    assert_eq!(inner.type_, basic(BasicType::I32));

    let source = emit_c(&lowered);
    assert!(source.contains("uint8_t gust_value = ((uint8_t)300);"));
    assert!(source.contains("int64_t gust_widened = ((int64_t)gust_value);"));
    assert!(source.contains("double gust_decimal = ((double)gust_widened);"));
    assert!(source.contains("static uint8_t gust_rt_f64_to_u8(double value)"));
    assert!(source.contains("static int32_t gust_rt_f64_to_i32(double value)"));
    assert!(source.contains("uint8_t gust_saturated = gust_rt_f64_to_u8(999.9);"));
    assert!(source.contains("int32_t gust_nan = gust_rt_f64_to_i32((0.0 / 0.0));"));
    assert!(source.contains("uint32_t gust_letter = ((uint32_t)65);"));
    assert!(source.contains("uint32_t gust_codePoint = ((uint32_t)gust_letter);"));
}

#[test]
fn logical_operators_lower_and_emit_native_c() {
    let result = check_source(
        r#"fn main() {
    let age: u32 = 30
    let enabled = true
    let disabled = false

    if age >= 18 && enabled && !disabled {
        io.println("access granted")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("logical operators should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("&&"));
    assert!(source.contains("(!gust_disabled)"));
    assert!(source.contains("(gust_age >= 18)"));
}

#[test]
fn logical_operators_preserve_short_circuiting_in_c() {
    let result = check_source(
        r#"fn shouldNotRun(): bool {
    io.println("unexpected")
    return true
}

fn main() {
    if false && shouldNotRun() {}
    if true || shouldNotRun() {
        io.println("done")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("logical operators should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("(false && gust_fn_"));
    assert!(source.contains("(true || gust_fn_"));
}

#[test]
fn logical_operators_lower_conditional_execution_statements() {
    let result = check_source(
        r#"fn truthy() => true
fn falsy() => false

fn main() {
    truthy() && io.println("and")
    falsy() || io.println("or")
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("conditional execution should lower");
    let LoweredStatement::If {
        condition,
        then_branch,
        else_branch,
    } = &lowered.statements[0]
    else {
        panic!("expected conditional execution to lower to an if statement");
    };
    assert!(matches!(condition.kind, LoweredExprKind::Call { .. }));
    assert!(matches!(then_branch.as_slice(), [LoweredStatement::Println(_)]));
    assert!(else_branch.is_none());

    let LoweredStatement::If {
        condition,
        then_branch,
        else_branch,
    } = &lowered.statements[1]
    else {
        panic!("expected conditional execution to lower to an if statement");
    };
    assert!(matches!(condition.kind, LoweredExprKind::Not(_)));
    assert!(matches!(then_branch.as_slice(), [LoweredStatement::Println(_)]));
    assert!(else_branch.is_none());
}
