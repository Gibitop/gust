use gustc::ast::Item;
use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lexer::Lexer;
use gustc::parser::Parser;

#[test]
fn hello_world_has_no_frontend_errors() {
    let source = include_str!("../../examples/hello-world.gust");
    let result = check_source(source);

    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basics_parses_without_syntax_errors() {
    let source = include_str!("../../examples/milestone.gust");
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
fn basics_reports_unsupported_features() {
    let source = include_str!("../../examples/milestone.gust");
    let result = check_source(source);

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Warning
                && diagnostic.message.contains("not implemented yet")),
        "expected at least one unsupported-feature warning, got {:?}",
        result.diagnostics
    );
}

#[test]
fn import_aliases_parse_and_define_the_local_name() {
    let source = r#"from package import { External as LocalExternal, helper as localHelper }

fn main() {
    localHelper()
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected aliased imports to validate, got {:?}",
        result.diagnostics
    );

    let Item::Import(import) = &result.program.items[0] else {
        panic!("expected import declaration");
    };
    assert_eq!(import.names[0].name, "External");
    assert_eq!(import.names[0].alias.as_deref(), Some("LocalExternal"));
    assert_eq!(import.names[1].name, "helper");
    assert_eq!(import.names[1].alias.as_deref(), Some("localHelper"));
}

#[test]
fn module_namespaces_parse_and_suppress_unknown_member_errors() {
    let source = r#"from package import package

fn main() {
    let value: package.Value = package.Value { name: "Gust" }
    package.print(value)
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected module namespace to validate, got {:?}",
        result.diagnostics
    );

    let Item::Import(import) = &result.program.items[0] else {
        panic!("expected import declaration");
    };
    assert!(import.names.is_empty());
    assert_eq!(
        import
            .namespace
            .as_ref()
            .map(|namespace| namespace.name.as_str()),
        Some("package")
    );
}

#[test]
fn basic_primitive_type_names_are_valid() {
    let result = check_source(
        r#"
fn main() {
    let string: String
    let boolean: bool
    let unsigned8: u8
    let unsigned16: u16
    let unsigned32: u32
    let unsigned64: u64
    let unsigned128: u128
    let pointerSized: usize
    let signed8: i8
    let signed16: i16
    let signed32: i32
    let signed64: i64
    let signed128: i128
    let float32: f32
    let float64: f64
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected basic primitive types to be valid, got {:?}",
        result.diagnostics
    );
}

#[test]
fn floating_point_literals_and_arithmetic_validate() {
    let result = check_source(
        r#"
fn main() {
    let single: f32 = 1.25
    let singleSum = single + 1.25
    let reverseSingleSum = 1.25 + single
    let double = 6.02e23
    let mixed = 1 + 2.5
    let remainder: f64 = 5.5 % 2
    let ordered = mixed < 4.0
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected floating-point expressions to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn floating_point_literals_do_not_initialize_integer_types() {
    let result = check_source(
        r#"
fn main() {
    let count: i128 = 1.5
}
"#,
    );

    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected value of type `i128`, got `f64`")),
        "expected integer initializer mismatch, got {:?}",
        result.diagnostics
    );
}

#[test]
fn bool_literals_validate_as_bool() {
    let result = check_source(
        r#"
fn main() {
    let enabled = true
    let disabled: bool = false
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected bool literals to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn if_else_statements_validate() {
    let result = check_source(
        r#"
fn main() {
    let enabled = true

    if enabled {
        io.println("enabled")
    } else if false {
        io.println("unreachable")
    } else {
        io.println("disabled")
    }
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected if/else statements to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn if_condition_must_be_bool() {
    let result = check_source(
        r#"
fn main() {
    if "not a bool" {}
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
                    .contains("expected value of type `bool`, got `String`")),
        "expected bool condition error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn if_branch_bindings_do_not_escape() {
    let result = check_source(
        r#"
fn main() {
    if true {
        let message = "scoped"
    }

    io.println(message)
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `message`")),
        "expected branch binding scope error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn while_statements_validate() {
    let result = check_source(
        r#"
fn main() {
    let mut index = 0

    while index < 5 {
        index += 1

        if index == 2 {
            continue
        }

        if index == 4 {
            break
        }
    }
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected while statement to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn while_condition_must_be_bool() {
    let result = check_source(
        r#"
fn main() {
    while "not a bool" {}
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
                    .contains("expected value of type `bool`, got `String`")),
        "expected bool condition error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn while_branch_bindings_do_not_escape() {
    let result = check_source(
        r#"
fn main() {
    while true {
        let message = "scoped"
        break
    }

    io.println(message)
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `message`")),
        "expected loop binding scope error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn break_and_continue_require_loop() {
    let result = check_source(
        r#"
fn main() {
    break
    continue
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
                    .contains("`break` can only be used inside a loop")),
        "expected break context error, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("`continue` can only be used inside a loop")),
        "expected continue context error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn returning_if_else_satisfies_explicit_return_type() {
    let result = check_source(
        r#"
fn choose(enabled: bool): String {
    if enabled {
        return "enabled"
    } else {
        return "disabled"
    }
}

fn main() {}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected returning if/else to satisfy return type, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basic_struct_literal_validates() {
    let result = check_source(
        r#"
struct Lang {
    name: String
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
fn struct_field_access_validates_as_field_type() {
    let result = check_source(
        r#"
struct Lang {
    name: String
    version: u32
}

fn main() {
    let lang = Lang {
        name: "Gust",
        version: 1,
    }
    let name: String = lang.name
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
    name: String
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
    name: String
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
    name: String
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
    name: String
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
                    .contains("expected value of type `u32`, got `String`")),
        "expected field type mismatch, got {:?}",
        result.diagnostics
    );
}

#[test]
fn struct_methods_validate_with_typed_self_and_arguments() {
    let result = check_source(
        r#"
struct Lang {
    name: String

    fn greeting(prefix: String): String {
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

    let u8Value: String = u8Number.toString()
    let u16Value: String = u16Number.toString()
    let u32Value: String = u32Number.toString()
    let u64Value: String = u64Number.toString()
    let u128Value: String = u128Number.toString()
    let usizeValue: String = usizeNumber.toString()
    let i8Value: String = i8Number.toString()
    let i16Value: String = i16Number.toString()
    let i32Value: String = i32Number.toString()
    let i64Value: String = i64Number.toString()
    let i128Value: String = i128Number.toString()
    let f32Value: String = f32Number.toString()
    let f64Value: String = f64Number.toString()
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
    name: String

    fn greeting(prefix: String): String {
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
                    .contains("expected value of type `String`, got `i32`")),
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
    name: String
}

fn Greeter.greeting(prefix: String) {
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

fn String.withSuffix(suffix: String): String {
    return self + suffix
}

fn External.label(): String {
    return "external"
}

fn Other.label(): String {
    return "other"
}

fn externalLabel(value: External): String {
    return value.label()
}

fn otherLabel(value: Other): String {
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
fn Missing.label(): String => "missing"
fn String.label(): String => self
fn String.label(): String => self
fn String.clone(): String => self

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
                    .contains("duplicate extension function `label` for type `String`")),
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
    name: String

    static fn new(name: String): Self => Self { name: name }
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
    static fn invalid(): String => self
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
                    .contains("expected value of type `u32`, got `String`")),
        "expected type-mismatch error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn string_add_validates_as_string() {
    let result = check_source(
        r#"
fn main() {
    let message: String = "Hello, " + "Gust"
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
    let message: String = "Hello, " + name + "!"
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
fn greet(name: String): String {
    return "Hello, " + name
}

fn main() {
    let message: String = greet("Gust")
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
fn greet(name: String): String {
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
fn greet(name: String): String {
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
                    .contains("expected value of type `String`, got `i32`")),
        "expected wrong-argument-type error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn function_return_type_mismatch_is_an_error() {
    let result = check_source(
        r#"
fn greet(): String {
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
                    .contains("expected value of type `String`, got `i32`")),
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
    let message: String = missing("Gust")
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
fn greet(name: String): String {
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
    label: String
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
            .contains("expected value of type `u32`, got `String`")),
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
    text: String
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
    text: String
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

#[test]
fn clone_creates_mutable_capability_from_immutable_structs() {
    let result = check_source(
        r#"
struct A {
    text: String
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
    text: String
}

fn read(value: A): String {
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
            .contains("operator + only supports numeric or String operands")),
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
                    .contains("expected value of type `String`, got `i32`")),
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
    name: String
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
                    .contains("operator == only supports numeric, bool, and String operands")),
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
                    .contains("expected value of type `bool`, got `String`")),
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
    name: String
}

enum Being {
    Person(Person)
    Unknown
}

fn greeting(being: Being): String {
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
fn enum_matches_must_be_exhaustive() {
    let result = check_source(
        r#"enum Status {
    Ready
    Waiting
}

fn label(status: Status): String {
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
    Value(String)
    Empty
}

fn label(result: Result): String {
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
                        .contains("expected value of type `String`, got `bool`")
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
    Named(String)
    Empty
}

fn label(state: State): String {
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
    Dog(String)
}

fn label(being: Being): String {
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
                    "`Being.Dog` contains a `String` value; use `Being.Dog(value)` to bind it or `Being.Dog(_)` to ignore it",
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
    Dog(String)
}

fn label(being: Being): String {
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
    Dog(String)
}

fn label(being: Being): String {
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
    Value(String)
}

enum Right {
    Value(String)
}

fn leftLabel(value: Left): String {
    return match value {
        Left.Value(label) => label,
    }
}

fn rightLabel(value: Right): String {
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

fn label(status: Status): String {
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
        r#"fn label(value: String): String {
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
        r#"fn label(value: String): String {
    return match value {
        "ready" => "Ready",
    }
}

fn main() {}"#,
    );
    assert!(missing_wildcard.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("non-exhaustive match for `String`; add a wildcard branch")
    }));

    let unreachable = check_source(
        r#"fn label(value: String): String {
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
    Person(String)
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
    let text = Box<String> { value: "Gust" }
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

fn makeNone(): Option<String> {
    return Option.None
}

fn main() {
    let inferred = Option.Some(42)
    let explicit = Option<String>.Some("Gust")
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
    let value = Option<i32, String>.Some(1)
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
            .contains("conflicting types `i32` and `String` were inferred for `T`")
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
    let text: Box<String> = Box { value: "Gust" }
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

fn makeEmpty(): Empty<String> => Empty.new()

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
            .contains("conflicting types `i32` and `String` were inferred for `T`")
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
    let text = identity<String>("Gust")
    let wrapped = some("wrapped")
    let box = boxed(9)
    let missing: Option<String> = none()
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
            .contains("conflicting types `i32` and `String` were inferred for `T`")
    }));

    let invalid_count = check_source(
        r#"fn identity<T>(value: T): T => value

fn main() {
    let value = identity<i32, String>(1)
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
    let staticPair = Box<i32>.make<String>(7, "static")
    let wrapped = number.wrap<String>("value")
    let empty: Option<String> = number.empty()
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
    value.identity<String, i32>("text")
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
    fn update(mut self, name: String) {
        self.name = name
    }
    static fn new(name: String) => Self { name: name }
}

trait Describe {
    fn describe(): String
    fn update(mut self, name: String): void
    static fn new(name: String): Self
}

struct Person {
    name: String
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
fn trait_impls_report_missing_extra_and_mismatched_methods() {
    let missing = check_source(
        r#"trait Describe {
    fn describe(): String
    static fn new(name: String): Self
}

struct Person {
    name: String
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
    fn describe(): String
    static fn new(name: String): Self
}

struct Person {
    name: String
}

impl Describe for Person {
    fn describe(): String => self.name
    static fn new(name: String) => Self { name: name }
    fn extra(): String => self.name
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
    fn describe(): String
    static fn new(name: String): Self
}

struct Person {
    name: String
}

impl Describe for Person {
    fn describe(): i32 => 1
    static fn new(name: String) => Self { name: name }
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
    fn describe(): String
    static fn new(name: String): Self
}

struct Person {
    name: String
}

impl Describe for Person {
    fn describe() => 1
    static fn new(name: String) => Self { name: name }
}

fn main() {}"#,
    );
    assert!(inferred_mismatch.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("expected value of type `String`, got `i32`")
    }));
}
