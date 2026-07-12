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
    name: string
}

enum Being {
    Person(Person)
    Unknown
}

fn greeting(being: Being): string {
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
fn mutable_enum_payload_patterns_validate_for_mutable_match_values() {
    let result = check_source(
        r#"struct StringContainer {
    value: string

    fn set(mut self, value: string) {
        self.value = value
    }
}

enum Option {
    Some(StringContainer)
    None

    fn set(mut self, value: string) {
        match self {
            Option.Some(mut container) => container.set(value),
            Option.None => {},
        }
    }
}

fn main() {
    let mut option = Option.Some(StringContainer { value: "Hello, World!" })
    option.set("Hello, Gust!")
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected mutable payload pattern to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_method_calls_on_match_payloads_suggest_mutable_payload_patterns() {
    let result = check_source(
        r#"struct StringContainer {
    value: string

    fn set(mut self, value: string) {
        self.value = value
    }
}

enum Option {
    Some(StringContainer)
    None

    fn set(mut self, value: string) {
        match self {
            Option.Some(container) => container.set(value),
            Option.None => {},
        }
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains(
                    "cannot call mutable function `StringContainer.set` through immutable match payload `container`; bind the payload as mutable with `Option.Some(mut container)`",
                )
        }),
        "expected mutable payload pattern suggestion, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_method_calls_on_immutable_match_payloads_do_not_suggest_invalid_mut_patterns() {
    let result = check_source(
        r#"struct StringContainer {
    value: string

    fn set(mut self, value: string) {
        self.value = value
    }
}

enum Option {
    Some(StringContainer)
    None
}

fn main() {
    let option = Option.Some(StringContainer { value: "Hello" })
    match option {
        Option.Some(container) => container.set("Hello, Gust!"),
        Option.None => {},
    }
}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains(
                    "cannot call mutable function `StringContainer.set` through immutable match payload `container`; `Option.Some(mut container)` requires matching a mutable value",
                )
        }),
        "expected immutable match value note, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_enum_payload_patterns_require_mutable_match_values() {
    let result = check_source(
        r#"struct StringContainer {
    value: string
}

enum Option {
    Some(StringContainer)
    None
}

fn main() {
    let container = StringContainer { value: "Hello" }
    let option = Option.Some(container)
    match option {
        Option.Some(mut value) => io.println(value.value),
        Option.None => {},
    }
}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("cannot bind mutable payload `value` from an immutable match value")
        }),
        "expected immutable match value error, got {:?}",
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

fn label(status: Status): string {
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
fn enum_or_patterns_validate_and_count_for_exhaustiveness() {
    let result = check_source(
        r#"enum Status {
    Ready
    Waiting
    Done
}

fn label(status: Status): string {
    return match status {
        Status.Ready | Status.Waiting => "active",
        Status.Done => "done",
    }
}

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected enum or-patterns to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn or_pattern_bindings_must_match_across_alternatives() {
    let result = check_source(
        r#"enum Result {
    Ok(string)
    Err(string)
}

fn label(result: Result): string {
    return match result {
        Result.Ok(text) | Result.Err(error) => text,
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("or-pattern alternatives must bind the same names")
        }),
        "expected or-pattern binding diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn duplicate_and_unreachable_checks_understand_or_patterns() {
    let duplicate = check_source(
        r#"enum Status {
    Ready
    Waiting
    Done
}

fn label(status: Status): string {
    return match status {
        Status.Ready | Status.Waiting => "active",
        Status.Ready => "ready",
        Status.Done => "done",
    }
}

fn main() {}"#,
    );

    assert!(
        duplicate.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("duplicate match branch for variant `Ready`")
        }),
        "expected duplicate variant diagnostic, got {:?}",
        duplicate.diagnostics
    );

    let unreachable = check_source(
        r#"enum Status {
    Ready
    Waiting
}

fn label(status: Status): string {
    return match status {
        Status.Ready | Status.Waiting => "active",
        Status.Ready => "ready",
    }
}

fn main() {}"#,
    );

    assert!(
        unreachable.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("match branches after a covering pattern are unreachable")
        }),
        "expected unreachable branch diagnostic, got {:?}",
        unreachable.diagnostics
    );
}

#[test]
fn enum_payloads_and_match_branches_are_type_checked() {
    let result = check_source(
        r#"enum Result {
    Value(string)
    Empty
}

fn label(result: Result): string {
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
                        .contains("expected value of type `string`, got `bool`")
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
    Named(string)
    Empty
}

fn label(state: State): string {
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
    Dog(string)
}

fn label(being: Being): string {
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
                    "`Being.Dog` contains a `string` value; use `Being.Dog(value)` to bind it or `Being.Dog(_)` to ignore it",
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
    Dog(string)
}

fn label(being: Being): string {
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
    Dog(string)
}

fn label(being: Being): string {
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
fn nested_enum_payload_patterns_validate_and_bind_inner_payloads() {
    let result = check_source(
        r#"enum Option {
    Some(Result)
    None
}

enum Result {
    Ok(string)
    Err(string)
}

fn label(value: Option): string {
    return match value {
        Option.Some(Result.Ok(text)) => text,
        Option.Some(Result.Err(error)) => error,
        Option.None => "none",
    }
}

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected nested enum payload patterns to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn wrong_nested_enum_payload_variant_reports_expected_enum() {
    let result = check_source(
        r#"enum Option {
    Some(Result)
    None
}

enum Result {
    Ok(string)
    Err(string)
}

enum Other {
    Ok(string)
}

fn label(value: Option): string {
    return match value {
        Option.Some(Other.Ok(text)) => text,
        Option.Some(Result.Err(error)) => error,
        Option.None => "none",
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("pattern `Other.Ok` does not belong to enum `Result`")
        }),
        "expected wrong nested variant diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn nested_underscore_payload_patterns_do_not_create_locals() {
    let result = check_source(
        r#"enum Option {
    Some(Result)
    None
}

enum Result {
    Ok(string)
    Err(string)
}

fn label(value: Option): string {
    return match value {
        Option.Some(Result.Ok(_)) => _,
        Option.Some(Result.Err(error)) => error,
        Option.None => "none",
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.message.contains("unknown name `_`")
        }),
        "expected nested underscore not to create a binding, got {:?}",
        result.diagnostics
    );
}

#[test]
fn nested_mutable_payload_bindings_require_mutable_match_values() {
    let result = check_source(
        r#"struct Box {
    value: string

    fn set(mut self, value: string) {
        self.value = value
    }
}

enum Option {
    Some(Result)
    None
}

enum Result {
    Ok(Box)
    Err(string)
}

fn update(mut value: Option, text: string) {
    match value {
        Option.Some(Result.Ok(mut box)) => box.set(text),
        Option.Some(Result.Err(_)) => {},
        Option.None => {},
    }
}

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected nested mutable payload binding to validate for mutable values, got {:?}",
        result.diagnostics
    );

    let result = check_source(
        r#"struct Box {
    value: string
}

enum Option {
    Some(Result)
    None
}

enum Result {
    Ok(Box)
    Err(string)
}

fn update(value: Option) {
    match value {
        Option.Some(Result.Ok(mut box)) => io.println(box.value),
        Option.Some(Result.Err(_)) => {},
        Option.None => {},
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("cannot bind mutable payload `box` from an immutable match value")
        }),
        "expected nested mutable payload binding to require a mutable value, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_patterns_validate_and_bind_fields() {
    let result = check_source(
        r#"struct Person {
    name: string
    age: i32
}

enum MaybePerson {
    Some(Person)
    None
}

fn shorthand(person: Person): string {
    return match person {
        Person { name, age } => name,
    }
}

fn renamed(person: Person): string {
    return match person {
        Person { name: personName, ... } => personName,
    }
}

fn payload(value: MaybePerson): string {
    return match value {
        MaybePerson.Some(Person { name, ... }) => name,
        MaybePerson.None => "none",
    }
}

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected struct patterns to validate and bind fields, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_patterns_report_field_errors() {
    let missing = check_source(
        r#"struct Person {
    name: string
    age: i32
}

fn name(person: Person): string {
    return match person {
        Person { name } => name,
    }
}

fn main() {}"#,
    );

    assert!(
        missing.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("struct pattern `Person` is missing field `age`")
        }),
        "expected missing field diagnostic, got {:?}",
        missing.diagnostics
    );

    let invalid_fields = check_source(
        r#"struct Person {
    name: string
    age: i32
}

fn name(person: Person): string {
    return match person {
        Person { name, name: otherName, height, ... } => name,
    }
}

fn main() {}"#,
    );

    assert!(
        invalid_fields.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("duplicate field `name` in struct pattern `Person`")
        }),
        "expected duplicate field diagnostic, got {:?}",
        invalid_fields.diagnostics
    );
    assert!(
        invalid_fields.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("unknown field `height` for struct `Person`")
        }),
        "expected unknown field diagnostic, got {:?}",
        invalid_fields.diagnostics
    );

    let wrong_type = check_source(
        r#"struct Person {
    name: string
    age: i32
}

fn name(person: Person): string {
    return match person {
        Person { name: 1, ... } => "Ada",
    }
}

fn main() {}"#,
    );

    assert!(
        wrong_type.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("numeric patterns cannot match a `string` value")
        }),
        "expected field type diagnostic, got {:?}",
        wrong_type.diagnostics
    );
}

#[test]
fn enum_variants_are_namespaced_by_their_enum() {
    let result = check_source(
        r#"enum Left {
    Value(string)
}

enum Right {
    Value(string)
}

fn leftLabel(value: Left): string {
    return match value {
        Left.Value(label) => label,
    }
}

fn rightLabel(value: Right): string {
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

fn label(status: Status): string {
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

#[test]
fn string_matches_support_literals_and_require_a_final_wildcard() {
    let result = check_source(
        r#"fn label(value: string): string {
    return match value {
        "ready" => "Ready",
        _ => "Unknown",
    }
}

fn main() {
    io.println(label("ready"))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected string patterns to validate, got {:?}",
        result.diagnostics
    );

    let missing_wildcard = check_source(
        r#"fn label(value: string): string {
    return match value {
        "ready" => "Ready",
    }
}

fn main() {}"#,
    );
    assert!(missing_wildcard.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("non-exhaustive match for `string`; add a wildcard branch")
    }));

    let unreachable = check_source(
        r#"fn label(value: string): string {
    return match value {
        _ => "Unknown",
        "ready" => "Ready",
    }
}

fn main() {}"#,
    );
    assert!(unreachable.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("match branches after a wildcard are unreachable")
    }));
}

#[test]
fn integer_and_bool_literal_patterns_validate() {
    let result = check_source(
        r#"fn codeLabel(code: u16): string {
    return match code {
        200 => "ok",
        400..=499 => "client error",
        _ => "other",
    }
}

fn wideLabel(value: u128): string {
    return match value {
        340282366920938463463374607431768211455 => "max",
        _ => "other",
    }
}

fn flagLabel(flag: bool): string {
    return match flag {
        true => "true",
        false => "false",
    }
}

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected integer and bool patterns to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn integer_patterns_require_wildcards_and_in_range_literals() {
    let missing_wildcard = check_source(
        r#"fn label(value: u8): string {
    return match value {
        1 => "one",
    }
}

fn main() {}"#,
    );

    assert!(
        missing_wildcard.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("non-exhaustive match for `u8`; add a wildcard branch")),
        "expected integer exhaustiveness diagnostic, got {:?}",
        missing_wildcard.diagnostics
    );

    let out_of_range = check_source(
        r#"fn label(value: u8): string {
    return match value {
        256 => "too large",
        _ => "other",
    }
}

fn main() {}"#,
    );

    assert!(
        out_of_range.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("numeric match patterns for `u8` require integer literals in range")),
        "expected range diagnostic, got {:?}",
        out_of_range.diagnostics
    );
}

#[test]
fn bool_patterns_are_exhaustive_only_when_both_values_are_covered() {
    let result = check_source(
        r#"fn label(value: bool): string {
    return match value {
        true => "yes",
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("non-exhaustive match for `bool`; cover `true` and `false` or add a wildcard branch")),
        "expected bool exhaustiveness diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn float_match_patterns_are_rejected() {
    let result = check_source(
        r#"fn label(value: f64): string {
    return match value {
        1.0 => "one",
        _ => "other",
    }
}

fn main() {}"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("numeric match patterns do not support floating-point match values")),
        "expected float pattern diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn block_match_branches_and_shared_string_rebinding_validate() {
    let result = check_source(
        r#"enum Being {
    Person(string)
    Unknown
}

fn makeBeing(): Being {
    return Being.Person("Ada")
}

fn main() {
    let mut name = ""
    match makeBeing() {
        Being.Person(personName) => {
            name = personName
        },
        Being.Unknown => {
            name = "stranger"
        },
    }
    io.println(name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected block matches and string rebinding to validate, got {:?}",
        result.diagnostics
    );
}
