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

