#[test]
fn concrete_and_generic_associated_types_validate() {
    let result = check_source(
        r#"trait Index<Key> {
    type Output
    fn index(key: Key): Self.Output
}

struct Box<T> {
    value: T
}

impl<T> Index<usize> for Box<T> {
    type Output: T
    fn index(key: usize): T => self.value
}

fn read<C: Index<usize>>(collection: C): C.Output {
    return collection.index(0)
}

fn main() {
    io.println(read(Box { value: 7 }).toString())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected associated types to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn associated_types_work_nested_in_generic_types_and_fields() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

enum Result<T, E> {
    Ok(T)
    Err(E)
}

trait Source {
    type Item
    fn option(): Option<Self.Item>
    fn result(): Result<Self.Item, string>
}

struct NumberSource {}

impl Source for NumberSource {
    type Item: i32
    fn option(): Option<i32> => Option.Some(1)
    fn result(): Result<i32, string> => Result.Ok(2)
}

struct Holder<T: Source> {
    value: T.Item
}

fn main() {
    let source: Source<type Item: i32> = NumberSource {}
    let holder = Holder<NumberSource> { value: 3 }
    source.option()
    source.result()
    io.println(holder.value.toString())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected nested associated types to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn trait_object_requires_associated_type_bindings_used_by_methods() {
    let result = check_source(
        r#"trait Producer {
    type Item
    fn next(): Self.Item
}

struct Counter {}

impl Producer for Counter {
    type Item: i32
    fn next(): i32 => 1
}

fn main() {
    let producer: Producer = Counter {}
}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains(
            "trait-typed value `Producer` must bind associated type `Producer.Item`",
        )
    }));
}

#[test]
fn associated_type_declarations_and_definitions_are_complete_and_unique() {
    let duplicate_decl = check_source(
        r#"trait Values {
    type Item
    type Item
}

struct Value {}

impl Values for Value {
    type Item: i32
}

fn main() {}"#,
    );
    assert!(duplicate_decl
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicate associated type `Item`")));

    let invalid_defs = check_source(
        r#"trait Values {
    type Item
    type Error
}

struct Value {}

impl Values for Value {
    type Item: i32
    type Item: string
    type Other: bool
}

fn main() {}"#,
    );
    assert!(invalid_defs.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("duplicate definition of associated type `Values.Item`")));
    assert!(invalid_defs.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("trait `Values` does not declare associated type `Other`")));
    assert!(invalid_defs.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("missing definition of associated type `Values.Error`")));
}

#[test]
fn associated_type_bindings_must_name_declared_types_once() {
    let result = check_source(
        r#"trait Producer {
    type Item
    fn next(): Self.Item
}

fn consume(value: Producer<type Item: i32, type Item: i32>) {}
fn reject(value: Producer<type Other: i32>) {}
fn main() {}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("duplicate binding for associated type `Producer.Item`")));
    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("trait `Producer` does not declare associated type `Other`")));
}

#[test]
fn unresolved_unknown_and_ambiguous_projections_are_diagnosed() {
    let result = check_source(
        r#"trait Left {
    type Item
}

trait Right {
    type Item
}

struct Value {}

impl Left for Value {
    type Item: i32
}

impl Right for Value {
    type Item: string
}

struct Other {}

fn ambiguous<T: Left + Right>(value: T): T.Item => value
fn unresolved(value: Other): Other.Item => value
fn unknown(value: Value): Value.Missing => value
fn main() {}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("ambiguous associated type projection `T.Item`")));
    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("cannot resolve associated type projection `Other.Item`")),
        "got {:?}",
        result.diagnostics
    );
    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown associated type projection `Value.Missing`")));
}

#[test]
fn impl_signatures_are_checked_after_associated_type_substitution() {
    let result = check_source(
        r#"trait Producer {
    type Item
    fn next(): Self.Item
}

struct Counter {}

impl Producer for Counter {
    type Item: i32
    fn next(): string => "wrong"
}

fn main() {}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("method `next` does not match trait `Producer<type Item: i32>`")));
}

#[test]
fn impls_cannot_differ_only_by_associated_type_definitions() {
    let result = check_source(
        r#"trait Choice {
    type Output
}

struct Value {}

impl Choice for Value {
    type Output: i32
}

impl Choice for Value {
    type Output: string
}

fn main() {}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("conflicting implementations of trait `Choice` for type `Value`")));
}

#[test]
fn associated_type_definitions_are_rejected_outside_trait_impls() {
    let result = check_source(
        r#"type Item: i32

fn main() {}"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("associated-type definitions are only allowed inside trait impls")));
}

#[test]
fn trait_objects_need_only_bindings_used_by_method_signatures() {
    let result = check_source(
        r#"trait Labeled {
    type Metadata
    fn label(): string
}

struct Value {}

impl Labeled for Value {
    type Metadata: i32
    fn label(): string => "value"
}

fn main() {
    let value: Labeled = Value {}
    io.println(value.label())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected unused associated type to remain unbound, got {:?}",
        result.diagnostics
    );
}
