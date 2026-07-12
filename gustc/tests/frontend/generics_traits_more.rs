#[test]
fn disjoint_generic_trait_impls_are_allowed() {
    let result = check_source(
        r#"trait Convert<T> {
    fn convert(): T
}

struct Box<T> {
    value: T
}

impl<T> Convert<i32> for Box<T> {
    fn convert() => 1
}

impl<T> Convert<string> for Box<T> {
    fn convert() => "value"
}

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected disjoint generic impls to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_trait_methods_use_expected_types_without_builtin_names() {
    let result = check_source(
        r#"trait Convert<T> {
    fn convert(): T
}

impl Convert<UserId> for string {
    fn convert() => UserId { value: self }
}

impl Convert<Label> for string {
    fn convert() => Label { value: self }
}

struct UserId {
    value: string
}

struct Label {
    value: string
}

fn readUserId(value: UserId): string {
    return value.value
}

fn main() {
    let raw = "Gust"
    let id: UserId = raw.convert()
    let label: Label = raw.convert()
    io.println(readUserId(raw.convert()))
    io.println(id.value)
    io.println(label.value)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic trait conversions to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn gust_defined_into_blanket_impl_uses_from_bound() {
    let source = include_str!("../../../examples/into.gust");
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected Gust-defined From and Into traits to validate, got {:?}",
        result.diagnostics
    );

    let missing_from = check_source(
        r#"trait From<T> {
    static fn from(value: T): Self
}

trait Into<T> {
    fn into(): T
}

impl<T, U: From<T>> Into<U> for T {
    fn into() => U.from(self)
}

struct Missing {
    value: string
}

fn main() {
    let missing: Missing = "value".into()
}"#,
    );
    assert!(missing_from.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("does not satisfy bound `Missing: From<string>`")
    }));
}

#[test]
fn generic_bounds_are_checked_at_concrete_use_sites() {
    let result = check_source(
        r#"struct Person {
    name: string
}

struct Number {
    value: i32
}

trait Named {
    fn name(): string
}

trait Labeled {
    fn label(): string
}

impl Named for Person {
    fn name() => self.name
}

impl Labeled for Person {
    fn label() => "person"
}

fn getName<T: Named + Labeled>(value: T): string {
    value.label()
    return value.name()
}

fn main() {
    let number = Number { value: 1 }
    io.println(getName(number))
}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("type `Number` does not satisfy bound `Number: Named`")
    }));
}

#[test]
fn generic_bounds_allow_member_resolution_after_specialization() {
    let result = check_source(
        r#"struct Person {
    name: string
}

trait Named {
    fn name(): string
}

impl Named for Person {
    fn name() => self.name
}

fn getName<T: Named>(value: T): string {
    return value.name()
}

fn main() {
    let person = Person { name: "Gust" }
    io.println(getName(person))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected bounded generic function to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_traits_report_invalid_declarations_and_arguments() {
    let invalid = check_source(
        r#"trait Named<T, T, U> {
    fn name(): T
}

fn main() {}"#,
    );
    assert!(invalid.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate type parameter `T` in trait `Named`")
    }));
    assert!(invalid.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("unused type parameter `U` in trait `Named`")
    }));

    let wrong_count = check_source(
        r#"trait Named<T> {
    fn name(): T
}

struct Person {
    name: string
}

impl Named<string, i32> for Person {
    fn name() => self.name
}

fn main() {}"#,
    );
    assert!(wrong_count.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("generic trait `Named` expects 1 type arguments, got 2")
    }));

    let invalid_impl = check_source(
        r#"trait Named<T> {
    fn name(): T
}

struct Person {
    name: string
}

impl<T, T, U> Named<T> for Person {
    fn name() => self.name
}

fn main() {}"#,
    );
    assert!(invalid_impl.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate type parameter `T` in impl `Named<T> for Person`")
    }));
    assert!(invalid_impl.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("unused type parameter `U` in impl `Named<T> for Person`")
    }));
}

#[test]
fn trait_typed_values_require_impls() {
    let result = check_source(
        r#"trait Describe {
    fn describe(): string
}

struct Person {
    name: string
}

fn main() {
    let person = Person { name: "Gust" }
    let described: Describe = person
}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("expected value of type `Describe`, got `Person`")
    }));
}

#[test]
fn trait_impls_report_missing_extra_and_mismatched_methods() {
    let missing = check_source(
        r#"trait Describe {
    fn describe(): string
    static fn new(name: string): Self
}

struct Person {
    name: string
}

impl Describe for Person {
}

fn main() {}"#,
    );
    assert!(missing.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("impl of trait `Describe` for type `Person` is missing method `describe`")
    }));
    assert!(missing.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("impl of trait `Describe` for type `Person` is missing static method `new`")
    }));

    let extra = check_source(
        r#"trait Describe {
    fn describe(): string
    static fn new(name: string): Self
}

struct Person {
    name: string
}

impl Describe for Person {
    fn describe(): string => self.name
    static fn new(name: string) => Self { name: name }
    fn extra(): string => self.name
}

fn main() {}"#,
    );
    assert!(extra.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("method `extra` is not declared in trait `Describe`")
    }));

    let explicit_mismatch = check_source(
        r#"trait Describe {
    fn describe(): string
    static fn new(name: string): Self
}

struct Person {
    name: string
}

impl Describe for Person {
    fn describe(): i32 => 1
    static fn new(name: string) => Self { name: name }
}

fn main() {}"#,
    );
    assert!(explicit_mismatch.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("method `describe` does not match trait `Describe` for type `Person`")
    }));

    let inferred_mismatch = check_source(
        r#"trait Describe {
    fn describe(): string
    static fn new(name: string): Self
}

struct Person {
    name: string
}

impl Describe for Person {
    fn describe() => 1
    static fn new(name: string) => Self { name: name }
}

fn main() {}"#,
    );
    assert!(inferred_mismatch.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("expected value of type `string`, got `i32`")
    }));
}
