#[test]
fn basic_struct_literal_validates() {
    let result = check_source(
        r#"
struct Lang {
    name: string
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: 1,
    }
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected basic struct literal to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn empty_struct_literal_validates() {
    let result = check_source(
        r#"
struct Token {
}

fn main() {
    let token = Token {}
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected empty struct literal to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_field_access_validates_as_field_type() {
    let result = check_source(
        r#"
struct Lang {
    name: string
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: 1,
    }
    let name: string = lang.name
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected struct field access to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_missing_field_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: string
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
    }
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
                    .contains("missing field `version` in struct literal `Lang`")),
        "expected missing-field error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_unknown_field_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: string
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: 1,
    }
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
                    .contains("unknown field `version` for struct `Lang`")),
        "expected unknown-field error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_duplicate_field_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: string
}

fn main() {
    let lang = Lang {
        name: "Gust",
        name: "Gust",
    }
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
                    .contains("duplicate field `name` in struct literal")),
        "expected duplicate-field error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_literal_field_type_mismatch_is_an_error() {
    let result = check_source(
        r#"
struct Lang {
    name: string
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: "1",
    }
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
        "expected field type mismatch, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_methods_validate_with_typed_self_and_arguments() {
    let result = check_source(
        r#"
struct Lang {
    name: string

    fn greeting(prefix: string): string {
        return prefix + self.name
    }
}

fn main() {
    let lang = Lang { name: "Gust" }
    io.println(lang.greeting("Hello, "))
}
"#,
    );

    assert!(
        result.diagnostics.is_empty(),
        "expected struct method to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn numeric_to_string_validates_for_every_numeric_type() {
    let result = check_source(
        r#"
fn main() {
    let u8Number: u8 = 1
    let u16Number: u16 = 2
    let u32Number: u32 = 3
    let u64Number: u64 = 4
    let u128Number: u128 = 5
    let usizeNumber: usize = 6
    let i8Number: i8 = 7
    let i16Number: i16 = 8
    let i32Number: i32 = 9
    let i64Number: i64 = 10
    let i128Number: i128 = 11
    let f32Number: f32 = 1.25
    let f64Number: f64 = 2.5

    let u8Value: string = u8Number.toString()
    let u16Value: string = u16Number.toString()
    let u32Value: string = u32Number.toString()
    let u64Value: string = u64Number.toString()
    let u128Value: string = u128Number.toString()
    let usizeValue: string = usizeNumber.toString()
    let i8Value: string = i8Number.toString()
    let i16Value: string = i16Number.toString()
    let i32Value: string = i32Number.toString()
    let i64Value: string = i64Number.toString()
    let i128Value: string = i128Number.toString()
    let f32Value: string = f32Number.toString()
    let f64Value: string = f64Number.toString()
}
"#,
    );

    assert!(
        result.diagnostics.is_empty(),
        "expected numeric toString calls to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn numeric_to_string_rejects_arguments() {
    let result = check_source(
        r#"
fn main() {
    1.toString(2)
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
                    .contains("method `i32.toString` expects 0 arguments, got 1")),
        "expected toString argument count error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_method_calls_report_unknown_methods_and_argument_mismatches() {
    let result = check_source(
        r#"
struct Lang {
    name: string

    fn greeting(prefix: string): string {
        return prefix + self.name
    }
}

fn main() {
    let lang = Lang { name: "Gust" }
    lang.missing()
    lang.greeting(1)
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
                    .contains("unknown method `missing` for struct `Lang`")),
        "expected unknown method error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("expected value of type `string`, got `i32`")),
        "expected method argument type error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_method_self_is_immutable() {
    let result = check_source(
        r#"
struct Counter {
    value: i32

    fn increment(): void {
        self.value++
    }
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
                    .contains("cannot mutate field of immutable binding `self`")),
        "expected immutable self error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_methods_reject_duplicate_and_reserved_names() {
    let result = check_source(
        r#"
struct Value {
    fn display(): void {}
    fn display(): void {}
    fn clone(): Value {
        return self
    }
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
                    .contains("duplicate method `display` in struct `Value`")),
        "expected duplicate method error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("method name `clone` is reserved")),
        "expected reserved method name error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_self_allows_member_and_extension_mutation() {
    let result = check_source(
        r#"
struct Counter {
    value: i32

    fn increment(mut self): void {
        self.value++
    }
}

fn Counter.add(mut self, amount: i32): void {
    self.value += amount
}

fn main() {
    let mut counter = Counter { value: 0 }
    counter.increment()
    counter.add(2)
}
"#,
    );

    assert!(
        result.diagnostics.is_empty(),
        "expected mutable receivers to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_self_call_on_immutable_binding_has_a_dedicated_error() {
    let result = check_source(
        r#"
struct Counter {
    value: i32

    fn increment(mut self): void {
        self.value++
    }
}

fn main() {
    let counter = Counter { value: 0 }
    counter.increment()
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains(
                    "cannot call mutable function `Counter.increment` through immutable binding `counter`; declare it with `let mut counter`"
                )),
        "expected dedicated immutable receiver error, got {:?}",
        result.diagnostics
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expects 1 arguments")),
        "receiver must not count as a call argument, got {:?}",
        result.diagnostics
    );
}

#[test]
fn mutable_self_rejects_type_annotations() {
    let result = check_source(
        r#"
struct Counter {
    value: i32

    fn increment(mut self: Self): void {
        self.value++
    }
}

fn main() {}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains(
                    "mutable receivers must be written `mut self` without a type annotation"
                )),
        "expected mutable receiver syntax error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn extension_functions_parse_and_validate_with_typed_self() {
    use gustc::ast::{FunctionBody, Item};

    let result = check_source(
        r#"
struct Greeter {
    name: string
}

fn Greeter.greeting(prefix: string) {
    return prefix + self.name
}

fn main() {
    let greeter = Greeter { name: "Gust" }
    io.println(greeter.greeting("Hello, "))
}
"#,
    );

    assert!(
        result.diagnostics.is_empty(),
        "expected extension function to validate, got {:?}",
        result.diagnostics
    );

    let Item::Extension(extension) = &result.program.items[1] else {
        panic!("expected extension declaration");
    };
    assert_eq!(extension.type_ref.name, "Greeter");
    assert_eq!(extension.function.name.as_deref(), Some("greeting"));
    assert!(matches!(extension.function.body, FunctionBody::Block(_)));
}

#[test]
fn extension_functions_validate_for_basic_and_imported_types() {
    let result = check_source(
        r#"
from package import { External, Other }

fn string.withSuffix(suffix: string): string {
    return self + suffix
}

fn External.label(): string {
    return "external"
}

fn Other.label(): string {
    return "other"
}

fn externalLabel(value: External): string {
    return value.label()
}

fn otherLabel(value: Other): string {
    return value.label()
}

fn main() {
    io.println("Gust".withSuffix("!"))
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected extensions on non-local types to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn extension_functions_reject_unknown_duplicate_and_reserved_declarations() {
    let result = check_source(
        r#"
fn Missing.label(): string => "missing"
fn string.label(): string => self
fn string.label(): string => self
fn string.clone(): string => self

fn main() {}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown type `Missing`")),
        "expected unknown extension type error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("duplicate extension function `label` for type `string`")),
        "expected duplicate extension error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("extension function name `clone` is reserved")),
        "expected reserved extension error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn static_functions_parse_and_validate_with_contextual_self() {
    use gustc::ast::{Item, StructMember};

    let result = check_source(
        r#"
struct Greeter {
    name: string

    static fn new(name: string): Self => Self { name: name }
}

static fn Greeter.default(): Self => Self.new("Gust")

fn main() {
    let greeter = Greeter.default()
    io.println(greeter.name)
}
"#,
    );

    assert!(
        result.diagnostics.is_empty(),
        "expected static functions to validate, got {:?}",
        result.diagnostics
    );

    let Item::Struct(struct_) = &result.program.items[0] else {
        panic!("expected struct");
    };
    assert!(matches!(
        &struct_.members[1],
        StructMember::StaticMethod(function)
            if function.name.as_deref() == Some("new")
    ));
    let Item::Extension(extension) = &result.program.items[1] else {
        panic!("expected static extension declaration");
    };
    assert!(extension.static_);
}

#[test]
fn static_functions_do_not_define_an_instance_self() {
    let result = check_source(
        r#"
struct Value {
    static fn invalid(): string => self
}

fn main() {}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `self`")),
        "expected static self error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_extensions_validate_for_blanket_and_concrete_instantiations() {
    let result = check_source(
        r#"struct Box<T> {
    value: T
}

fn Box<T>.get(): T => self.value
fn Box<i32>.label(): string => "integer"

fn main() {
    let text = Box { value: "Gust" }
    let number = Box { value: 42 }

    io.println(text.get())
    io.println(number.get().toString())
    io.println(number.label())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic extensions to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_static_extension_functions_validate_when_selected() {
    let result = check_source(
        r#"struct Box<T> {
    value: T
}

struct Pair<T, U> {
    first: T
    second: U
}

static fn Box<T>.pair<U>(value: T, other: U): Pair<T, U> => Pair {
    first: value,
    second: other,
}

fn main() {
    let pair = Box<i32>.pair<string>(7, "seven")

    io.println(pair.second)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected generic static extension function to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn generic_extension_receiver_bounds_are_checked_at_selected_calls() {
    let valid = check_source(
        r#"trait Named {
    fn name(): string
}

struct Person {
    value: string
}

impl Named for Person {
    fn name(): string => self.value
}

struct Box<T> {
    value: T
}

fn Box<T: Named>.name(): string => self.value.name()

fn main() {
    let person = Box { value: Person { value: "Gust" } }

    io.println(person.name())
}"#,
    );

    assert!(
        !valid.has_errors(),
        "expected bounded generic extension to validate, got {:?}",
        valid.diagnostics
    );

    let invalid = check_source(
        r#"trait Named {
    fn name(): string
}

struct Box<T> {
    value: T
}

fn Box<T: Named>.name(): string => self.value.name()

fn main() {
    let number = Box { value: 42 }

    io.println(number.name())
}"#,
    );

    assert!(
        invalid.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("type `i32` does not satisfy bound `i32: Named`")),
        "expected bounded generic extension call to report a bound error, got {:?}",
        invalid.diagnostics
    );
}

#[test]
fn unknown_type_names_are_errors() {
    let result = check_source(
        r#"
fn main() {
    let value: Nope = 1
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
}

#[test]
fn unknown_type_suppresses_followup_initializer_mismatch() {
    let result = check_source(
        r#"
fn main() {
    let value: Nope = "not checked again"
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
        result
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("expected value of type")),
        "expected Unknown to suppress mismatch cascades, got {:?}",
        result.diagnostics
    );
}
