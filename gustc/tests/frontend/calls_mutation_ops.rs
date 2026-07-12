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
                    .contains("expected value of type `u32`, got `string`")),
        "expected type-mismatch error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn string_add_validates_as_string() {
    let result = check_source(
        r#"
fn main() {
    let message: string = "Hello, " + "Gust"
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
    let message: string = "Hello, " + name + "!"
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
fn greet(name: string): string {
    return "Hello, " + name
}

fn main() {
    let message: string = greet("Gust")
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
fn greet(name: string): string {
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
fn greet(name: string): string {
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
                    .contains("expected value of type `string`, got `i32`")),
        "expected wrong-argument-type error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn function_return_type_mismatch_is_an_error() {
    let result = check_source(
        r#"
fn greet(): string {
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
                    .contains("expected value of type `string`, got `i32`")),
        "expected return-type mismatch error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn inferred_return_function_values_validate_with_context() {
    let result = check_source(
        r#"
fn apply(value: i32, f: fn(i32): i32) {
    return f(value)
}

fn addOne(value: i32) {
    return value + 1
}

fn main() {
    let result = apply(41, addOne)
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected inferred return function value to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_call_callee_suppresses_initializer_mismatch() {
    let result = check_source(
        r#"
fn main() {
    let message: string = missing("Gust")
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
fn greet(name: string): string {
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
fn mutable_struct_fields_can_be_assigned_compounded_and_incremented() {
    let result = check_source(
        r#"
struct State {
    count: u32
    flags: u8
    label: string
}

fn main() {
    let mut state = State {
        count: 20,
        flags: 1,
        label: "state",
    }
    state.count = 24
    state.count += 4
    state.count -= 2
    state.count *= 3
    state.count /= 2
    state.count %= 5
    state.count++
    state.flags |= 2
    state.flags &= 3
    state.flags ^= 1
    state.flags <<= 2
    state.flags >>= 1
    state.label += " updated"
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected mutable struct field operations to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_field_mutation_requires_a_mutable_binding() {
    let result = check_source(
        r#"
struct State {
    count: u32
}

fn main() {
    let state = State { count: 1 }
    state.count = 2
    state.count++
}
"#,
    );

    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic
                .message
                .contains("cannot mutate field of immutable binding `state`"))
            .count(),
        2,
        "expected immutable-field errors, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_field_mutation_uses_field_type_rules() {
    let result = check_source(
        r#"
struct State {
    count: u32
    enabled: bool
}

fn main() {
    let mut state = State {
        count: 1,
        enabled: true,
    }
    state.count = "many"
    state.enabled++
}
"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected value of type `u32`, got `string`")),
        "expected field assignment type error, got {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("operator ++ only supports numeric operands, got `bool`")),
        "expected field increment type error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_field_mutation_rejects_computed_struct_values() {
    let result = check_source(
        r#"
struct State {
    count: u32
}

fn makeState(): State {
    return State { count: 1 }
}

fn main() {
    makeState().count = 2
    makeState().count++
}
"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("field assignment target must be rooted in")),
        "expected rooted-field assignment error, got {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("increment target must be rooted in")),
        "expected rooted-field increment error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn nested_struct_fields_can_be_mutated_through_a_mutable_root() {
    let result = check_source(
        r#"
struct State {
    flags: Flags
}

struct Flags {
    enabled: bool
    count: u32
}

fn main() {
    let mut state = State {
        flags: Flags {
            enabled: false,
            count: 1,
        },
    }
    state.flags.enabled = true
    state.flags.count += 2
    state.flags.count++
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected nested struct field mutation to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn nested_struct_field_mutation_requires_a_mutable_root() {
    let result = check_source(
        r#"
struct State {
    flags: Flags
}

struct Flags {
    enabled: bool
}

fn main() {
    let state = State {
        flags: Flags { enabled: false },
    }
    state.flags.enabled = true
}
"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("cannot mutate field of immutable binding `state`")),
        "expected immutable-root error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn immutable_struct_references_cannot_gain_mutable_capability() {
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
    let mut invalidA = immutableA
    let mut invalidB = B { a: immutableA }
    mutate(immutableB)
}
"#,
    );

    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("immutable value"))
            .count(),
        2,
        "expected immutable-to-mutable initialization errors, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("requires a mutable argument")),
        "expected mutable-argument error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn constructor_calls_preserve_argument_mutability() {
    let result = check_source(
        r#"
struct A {
    text: string
}

struct B {
    a: A

    static fn new(a: A): Self => Self { a: a }
}

fn main() {
    let mut mutableA = A { text: "mutable" }
    let mut validB = B.new(mutableA)
    let immutableA = A { text: "immutable" }
    let mut invalidB = B.new(immutableA)
}
"#,
    );

    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("immutable value"))
            .count(),
        1,
        "expected only the immutable constructor argument to fail, got {:?}",
        result.diagnostics
    );
}

