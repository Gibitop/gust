#[test]
fn generic_structs_validate_each_concrete_specialization() {
    let result = check_source(
        r#"struct Box<T> {
    value: T

    fn get(): T {
        return self.value
    }
}

struct Pair<T> {
    first: T
    second: T
}

fn main() {
    let number = Box<i32> { value: 42 }
    let text = Box<string> { value: "Gust" }
    let pair = Pair<Box<i32>> {
        first: number,
        second: Box<i32> { value: 7 },
    }
    io.println(text.get())
    io.println(pair.second.get().toString())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic structs to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_enums_infer_and_validate_concrete_specializations() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

fn unwrapOr(value: Option<i32>, fallback: i32): i32 {
    return match value {
        Option.Some(inner) => inner,
        Option.None => fallback,
    }
}

fn makeNone(): Option<string> {
    return Option.None
}

fn main() {
    let inferred = Option.Some(42)
    let explicit = Option<string>.Some("Gust")
    let contextual: Option<i32> = Option.None
    let text = makeNone()
    io.println(unwrapOr(inferred, 0).toString())
    io.println(unwrapOr(contextual, 7).toString())
    match explicit {
        Option.Some(inner) => io.println(inner),
        Option.None => io.println("missing"),
    }
    match text {
        Option.Some(inner) => io.println(inner),
        Option.None => io.println("missing"),
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic enums to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_enum_expected_types_flow_into_calls_and_nested_payloads() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

enum Holder<T> {
    Item(T)
}

fn consume(value: Option<i32>) {}

fn main() {
    consume(Option.None)
    let nested: Holder<Option<i32>> = Holder.Item(Option.None)
    match nested {
        Holder.Item(option) => match option {
            Option.Some(value) => io.println(value.toString()),
            Option.None => io.println("missing"),
        },
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic enum contexts to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_enums_report_unresolved_and_invalid_type_arguments() {
    let unresolved = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

fn main() {
    let value = Option.None
}"#,
    );
    assert!(unresolved.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot infer type arguments for generic enum `Option`")
    }));

    let wrong_count = check_source(
        r#"enum Option<T> {
    Some(T)
}

fn main() {
    let value = Option<i32, string>.Some(1)
}"#,
    );
    assert!(wrong_count.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("generic enum `Option` expects 1 type arguments, got 2")
    }));

    let duplicate = check_source(
        r#"enum Result<T, T> {
    Ok(T)
}

fn main() {
    let value = Result<i32, i32>.Ok(1)
}"#,
    );
    assert!(duplicate.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate type parameter `T` in enum `Result`")
    }));

    let conflicting = check_source(
        r#"struct Pair<First, Second> {
    first: First
    second: Second
}

enum Same<T> {
    Value(Pair<T, T>)
}

fn main() {
    let value = Same.Value(Pair { first: 1, second: "two" })
}"#,
    );
    assert!(conflicting.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting types `i32` and `string` were inferred for `T`")
    }));
}

#[test]
fn generic_function_values_infer_from_function_contexts_or_require_explicit_arguments() {
    let inferred = check_source(
        r#"fn identity<T>(value: T): T => value

fn apply<T>(value: T, transform: fn(T): T): T => transform(value)

fn main() {
    let result = apply(41, identity)
    io.println(result.toString())
}"#,
    );
    assert!(
        !inferred.has_errors(),
        "expected a contextual generic function value to validate, got {:?}",
        inferred.diagnostics
    );

    let explicit = check_source(
        r#"fn identity<T>(value: T): T => value

fn main() {
    let numberIdentity = identity<i32>
    io.println(numberIdentity(42).toString())
}"#,
    );
    assert!(
        !explicit.has_errors(),
        "expected an explicit generic function value to validate, got {:?}",
        explicit.diagnostics
    );

    let ambiguous = check_source(
        r#"fn identity<T>(value: T): T => value

fn main() {
    let value = identity
}"#,
    );
    assert!(ambiguous.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains(
            "cannot infer type arguments for generic function value `identity` without an expected function type",
        )
    }));
}

#[test]
fn generic_structs_require_the_declared_type_argument_count() {
    let missing = check_source(
        r#"struct Box<T> {
    marker: i32
}

fn main() {
    let value = Box { marker: 1 }
}"#,
    );
    assert!(missing.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot infer type arguments for generic struct `Box`")
    }));

    let duplicate = check_source(
        r#"struct Pair<T, T> {
    value: T
}

fn main() {
    let value = Pair<i32, i32> { value: 1 }
}"#,
    );
    assert!(duplicate.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate type parameter `T` in struct `Pair`")
    }));
}

#[test]
fn generic_static_calls_and_contextual_struct_literals_validate() {
    let result = check_source(
        r#"struct Box<T> {
    value: T

    static fn new(value: T): Self => Self { value: value }
}

fn main() {
    let number = Box.new(42)
    let inferred = Box { value: "inferred" }
    let text: Box<string> = Box { value: "Gust" }
    io.println(number.value.toString())
    io.println(inferred.value)
    io.println(text.value)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic construction to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_inference_uses_typed_locals_and_expected_return_types() {
    let result = check_source(
        r#"struct Empty<T> {
    marker: i32

    static fn new(): Self => Self { marker: 0 }
}

struct Box<T> {
    value: T

    static fn new(value: T): Self => Self { value: value }
}

fn makeEmpty(): Empty<string> => Empty.new()

fn main() {
    let value: u32 = 1
    let box = Box.new(value)
    let empty: Empty<bool> = Empty.new()
    io.println(box.value.toString())
    io.println(empty.marker.toString())
    io.println(makeEmpty().marker.toString())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected contextual generic inference to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_inference_reports_ambiguous_and_conflicting_arguments() {
    let ambiguous = check_source(
        r#"struct Empty<T> {
    marker: i32

    static fn new(): Self => Self { marker: 0 }
}

fn main() {
    let value = Empty.new()
}"#,
    );
    assert!(ambiguous.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("no concrete type was found for `T`")
    }));

    let conflicting = check_source(
        r#"struct Pair<T> {
    first: T
    second: T
}

fn main() {
    let value = Pair {
        first: 1,
        second: "two",
    }
}"#,
    );
    assert!(conflicting.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting types `i32` and `string` were inferred for `T`")
    }));
}

#[test]
fn generic_functions_infer_explicit_and_expected_type_arguments() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

struct Box<T> {
    value: T
}

fn identity<T>(value: T) => value
fn forward<T>(value: T) {
    return identity(value)
}
fn some<T>(value: T) {
    return Option.Some(value)
}
fn boxed<T>(value: T) {
    return Box { value: value }
}
fn none<T>() => Option<T>.None

fn main() {
    let number = identity(42)
    let nested = identity(identity(7))
    let forwarded = forward("forwarded")
    let text = identity<string>("Gust")
    let wrapped = some("wrapped")
    let box = boxed(9)
    let missing: Option<string> = none()
    if identity<bool>(true) {
        io.println("explicit")
    }
    io.println(number.toString())
    io.println(nested.toString())
    io.println(forwarded)
    io.println(text)
    io.println(box.value.toString())
    match wrapped {
        Option.Some(value) => io.println(value),
        Option.None => io.println("missing"),
    }
    match missing {
        Option.Some(value) => io.println(value),
        Option.None => io.println("missing"),
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic functions to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn recursive_generic_functions_are_specialized_once() {
    let result = check_source(
        r#"fn recurse<T>(value: T) {
    if false {
        return recurse<T>(value)
    }
    return value
}

fn main() {
    io.println(recurse("done"))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected recursive generic function to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unused_generic_function_bodies_are_not_instantiated() {
    let result = check_source(
        r#"fn unused<T>(value: T): T => value.missing()

fn main() {}"#,
    );

    assert!(
        !result.has_errors(),
        "expected unused generic function body to remain uninstantiated, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_functions_report_inference_and_declaration_errors() {
    let unresolved = check_source(
        r#"fn make<T>(): T {
    return make<T>()
}

fn main() {
    let value = make()
}"#,
    );
    assert!(unresolved.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot infer type arguments for generic function `make`")
    }));

    let conflicting = check_source(
        r#"fn same<T>(first: T, second: T): T => first

fn main() {
    let value = same(1, "two")
}"#,
    );
    assert!(conflicting.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting types `i32` and `string` were inferred for `T`")
    }));

    let invalid_count = check_source(
        r#"fn identity<T>(value: T): T => value

fn main() {
    let value = identity<i32, string>(1)
}"#,
    );
    assert!(invalid_count.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("generic function `identity` expects 1 type arguments, got 2")
    }));

    let declarations = check_source(
        r#"fn invalid<T, T, U>(value: T): T => value

fn main() {}"#,
    );
    assert!(declarations.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate type parameter `T` in function `invalid`")
    }));
    assert!(declarations.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("unused type parameter `U` in function `invalid`")
    }));
}

#[test]
fn generic_methods_infer_explicit_and_expected_type_arguments() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

struct Pair<A, B> {
    first: A
    second: B
}

struct Box<T> {
    value: T

    static fn make<U>(value: T, other: U) => Pair { first: value, second: other }

    fn pair<U>(other: U) => Pair { first: self.value, second: other }

    fn wrap<U>(other: U) => Option<U>.Some(other)

    fn empty<U>() => Option<U>.None
}

fn main() {
    let number = Box { value: 42 }
    let pair = number.pair("answer")
    let staticPair = Box<i32>.make<string>(7, "static")
    let wrapped = number.wrap<string>("value")
    let empty: Option<string> = number.empty()
    io.println(pair.second)
    io.println(staticPair.second)
    match wrapped {
        Option.Some(value) => io.println(value),
        Option.None => io.println("missing"),
    }
    match empty {
        Option.Some(value) => io.println(value),
        Option.None => io.println("empty"),
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic methods to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn enum_methods_validate_with_self_and_generic_payloads() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None

    fn unwrapOr(fallback: T): T {
        return match self {
            Option.Some(value) => value,
            Option.None => fallback,
        }
    }
}

fn main() {
    let present = Option.Some(42)
    let absent: Option<i32> = Option.None
    let value = present.unwrapOr(0)
    let fallback = absent.unwrapOr(7)
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected enum methods to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_methods_report_inference_and_declaration_errors() {
    let unresolved = check_source(
        r#"struct Box<T> {
    value: T

    fn choose<U>(): U => self.value
}

fn main() {
    let value = Box { value: 1 }
    value.choose()
}"#,
    );
    assert!(unresolved.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot infer type arguments for generic method")
    }));

    let invalid_count = check_source(
        r#"struct Box<T> {
    value: T

    fn identity<U>(value: U): U => value
}

fn main() {
    let value = Box { value: 1 }
    value.identity<string, i32>("text")
}"#,
    );
    assert!(invalid_count.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("generic method `Box<i32>.identity` expects 1 type arguments, got 2")
    }));

    let declarations = check_source(
        r#"struct Box<T> {
    value: T

    fn invalid<T, T, U>(value: T): T => value
}

fn main() {}"#,
    );
    assert!(declarations.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("duplicate type parameter `T` in method `invalid`")
    }));
    assert!(declarations.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("type parameter `T` in method `invalid` conflicts with struct `Box`")
    }));
    assert!(declarations.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("unused type parameter `U` in method `invalid`")
    }));
}

#[test]
fn traits_validate_and_dispatch_concrete_impl_methods() {
    let result = check_source(
        r#"impl Describe for Person {
    fn describe() => self.name
    fn update(mut self, name: string) {
        self.name = name
    }
    static fn new(name: string) => Self { name: name }
}

trait Describe {
    fn describe(): string
    fn update(mut self, name: string): void
    static fn new(name: string): Self
}

struct Person {
    name: string
}

fn main() {
    let mut person = Person.new("Gust")
    person.update("John")
    io.println(person.describe())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected trait impl to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn trait_typed_values_accept_implemented_concrete_values() {
    let result = check_source(
        r#"impl Describe for Person {
    fn describe() => self.name
}

trait Describe {
    fn describe(): string
}

struct Person {
    name: string
}

fn printDescription(value: Describe) {
    io.println(value.describe())
}

fn main() {
    let person = Person { name: "Gust" }
    let described: Describe = person
    printDescription(person)
    io.println(described.describe())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected trait typed values to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_traits_validate_concrete_specializations() {
    let result = check_source(
        r#"impl Named<string> for Person {
    fn name() => self.name
}

trait Named<T> {
    fn name(): T
}

struct Person {
    name: string
}

fn printName(value: Named<string>) {
    io.println(value.name())
}

fn main() {
    let person = Person { name: "Gust" }
    let named: Named<string> = person
    printName(person)
    io.println(named.name())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic trait specialization to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_trait_impl_templates_validate_concrete_specializations() {
    let result = check_source(
        r#"struct Box<T> {
    value: T
}

trait Named<T> {
    fn name(): T
}

impl<T> Named<T> for Box<T> {
    fn name() => self.value
}

fn printName(value: Named<string>) {
    io.println(value.name())
}

fn main() {
    let value = Box<string> { value: "Gust" }
    let named: Named<string> = value
    printName(value)
    io.println(named.name())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic trait impl template to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn overlapping_trait_impls_are_rejected_before_specialization() {
    let concrete_overlap = check_source(
        r#"trait Describe {
    fn describe(): string
}

struct Person {
    name: string
}

impl<T> Describe for T {
    fn describe() => "value"
}

impl Describe for Person {
    fn describe() => self.name
}

fn main() {}"#,
    );
    assert!(concrete_overlap.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting implementations of trait `Describe` for type `Person`")
    }));

    let bounded_overlap = check_source(
        r#"trait Named {
    fn name(): string
}

trait Labeled {
    fn label(): string
}

trait Describe {
    fn describe(): string
}

impl<T: Named> Describe for T {
    fn describe() => self.name()
}

impl<T: Labeled> Describe for T {
    fn describe() => self.label()
}

fn main() {}"#,
    );
    assert!(bounded_overlap.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting implementations of trait `Describe` for type `T`")
    }));

    let nested_overlap = check_source(
        r#"trait Describe {
    fn describe(): string
}

struct Box<T> {
    value: T
}

impl<T> Describe for T {
    fn describe() => "value"
}

impl<T> Describe for Box<T> {
    fn describe() => "box"
}

fn main() {}"#,
    );
    assert!(nested_overlap.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting implementations of trait `Describe` for type `Box<T>`")
    }));
}
