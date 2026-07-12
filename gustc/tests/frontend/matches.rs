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

