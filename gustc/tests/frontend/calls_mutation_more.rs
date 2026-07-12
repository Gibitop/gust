#[test]
fn clone_creates_mutable_capability_from_immutable_structs() {
    let result = check_source(
        r#"
struct A {
    text: string
}

struct B {
    a: A
}

fn mutate(mut value: B): void {
    value.a.text += "!"
}

fn main() {
    let immutableA = A { text: "immutable" }
    let immutableB = B { a: immutableA }
    let mut mutableA = immutableA.clone()
    let mut mutableB = immutableB.clone()
    mutableA.text += " copy"
    mutate(mutableB)
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected cloned structs to gain mutable capability, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_struct_references_can_be_viewed_as_immutable() {
    let result = check_source(
        r#"
struct A {
    text: string
}

fn read(value: A): string {
    return value.text
}

fn main() {
    let mut mutableA = A { text: "mutable" }
    let immutableView = mutableA
    mutableA.text += " updated"
    let text = read(mutableA)
    let viewedText = read(immutableView)
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected mutable-to-immutable views to validate, got {:?}",
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
            .contains("operator + only supports numeric or string operands")),
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
                    .contains("expected value of type `string`, got `i32`")),
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
    name: string
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
                    .contains("operator == only supports numeric, bool, and string operands")),
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
                    .contains("expected value of type `bool`, got `string`")),
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
