#[test]
fn mutable_local_assignment_and_increment_emit_c() {
    let result = check_source(
        r#"fn main() {
    let mut count: u32 = 1
    count = count + 2
    count++
    if count == 4 {
        count = 5
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("mutable locals should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("uint32_t gust_count = 1;"));
    assert!(source.contains("gust_count = (gust_count + 2);"));
    assert!(source.contains("(gust_count++);"));
    assert!(source.contains("gust_count = 5;"));
}

#[test]
fn mutable_struct_field_operations_emit_c() {
    let result = check_source(
        r#"struct State {
    count: u32
    flags: u8
    label: string
}

fn main() {
    let mut state = State {
        count: 1,
        flags: 1,
        label: "state",
    }
    state.count = 2
    state.count += 3
    state.flags |= 2
    state.label += " updated"
    state.count++
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("mutable struct fields should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("gust_state->gust_count = 2;"));
    assert!(source.contains("gust_state->gust_count = (gust_state->gust_count + 3);"));
    assert!(source.contains("gust_state->gust_flags = (gust_state->gust_flags | 2);"));
    assert!(source.contains(
        "gust_state->gust_label = gust_rt_string_concat(gust_state->gust_label, (gust_rt_string){"
    ));
    assert!(source.contains("(gust_state->gust_count++);"));
}

#[test]
fn nested_struct_fields_and_mutation_emit_pointer_access() {
    let result = check_source(
        r#"struct State {
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
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("nested struct fields should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("gust_state->gust_flags->gust_enabled = true;"));
    assert!(source.contains(
        "gust_state->gust_flags->gust_count = (gust_state->gust_flags->gust_count + 2);"
    ));
    assert!(source.contains("(gust_state->gust_flags->gust_count++);"));
}

#[test]
fn struct_assignment_aliases_and_clone_deep_copies() {
    let result = check_source(
        r#"struct A {
    text: string
}

struct Pair {
    first: A
    second: A
}

fn mutate(mut pair: Pair): void {
    pair.first.text += " shared"
}

fn main() {
    let mut value = A { text: "Gust" }
    let mut pair = Pair {
        first: value,
        second: value,
    }
    let view = pair
    mutate(pair)
    let mut copy = view.clone()
    copy.first.text += " clone"
    io.println(pair.second.text)
    io.println(copy.second.text)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("aliases and clone should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("gust_struct_"));
    assert!(source.contains("* gust_view = gust_pair;"));
    assert!(source.contains("gust_rt_clone_gust_struct_"));
    assert!(source.contains("gust_rt_clone_lookup"));
    assert!(source.contains("gust_rt_clone_register"));
    assert!(source.contains("result->gust_first = gust_rt_clone_"));
    assert!(source.contains("result->gust_second = gust_rt_clone_"));
}

#[test]
fn compound_assignments_emit_c() {
    let result = check_source(
        r#"fn main() {
    let mut count: i32 = 20
    count += 4
    count -= 2
    count *= 3
    count /= 2
    count %= 5
    let mut message = "hello"
    message += " world"
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("compound assignments should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("gust_count = (gust_count + 4);"));
    assert!(source.contains("gust_count = (gust_count - 2);"));
    assert!(source.contains("gust_count = (gust_count * 3);"));
    assert!(source.contains("gust_count = (gust_count / 2);"));
    assert!(source.contains("gust_count = (gust_count % 5);"));
    assert!(
        source.contains("gust_message = gust_rt_string_concat(gust_message, (gust_rt_string){")
    );
}

#[test]
fn bitwise_shift_and_compound_assignments_emit_c() {
    let result = check_source(
        r#"fn main() {
    let value: u32 = 1 | 2 ^ 3 & 4 << 1 + 1
    let shifted = value >> 2
    let mut flags: u8 = 1
    flags &= 7
    flags |= 2
    flags ^= 1
    flags <<= 2
    flags >>= 1
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("bitwise operations should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("uint32_t gust_value = (1 | (2 ^ (3 & (4 << (1 + 1)))));"));
    assert!(source.contains("uint32_t gust_shifted = (gust_value >> 2);"));
    assert!(source.contains("gust_flags = (gust_flags & 7);"));
    assert!(source.contains("gust_flags = (gust_flags | 2);"));
    assert!(source.contains("gust_flags = (gust_flags ^ 1);"));
    assert!(source.contains("gust_flags = (gust_flags << 2);"));
    assert!(source.contains("gust_flags = (gust_flags >> 1);"));
}

#[test]
fn mutable_struct_parameters_lower_as_shared_references() {
    let result = check_source(
        r#"
struct Person {
    name: string
}

fn rename(mut person: Person): void {
    person.name += "!"
}

fn main() {
    let mut person = Person { name: "Gust" }
    rename(person)
    io.println(person.name)
}
"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("mutable parameter should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("_Person* gust_person)"));
    assert!(source.contains(
        "gust_person->gust_name = gust_rt_string_concat(gust_person->gust_name, (gust_rt_string){"
    ));
    assert!(source.contains("gust_fn_"));
    assert!(source.contains("(gust_person);"));
}

#[test]
fn unknown_println_local_is_frontend_error() {
    let result = check_source(
        r#"fn main() {
    io.println(message)
}"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `message`")),
        "expected frontend unknown-name diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basics_reaches_build_mode_rejection() {
    let source = include_str!("../../../examples/milestone.gust");
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected basics frontend to avoid hard errors, got {:?}",
        result.diagnostics
    );

    let diagnostics = lower_program(&result.program).expect_err("basics should not lower");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("imports are not supported in executable builds")),
        "expected unsupported-import diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn struct_methods_lower_to_functions_with_self_receivers() {
    let result = check_source(
        r#"struct Lang {
    name: string

    fn greeting(prefix: string) {
        return prefix + self.name
    }
}

fn main() {
    let lang = Lang { name: "Gust" }
    io.println(lang.greeting("Hello, "))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("struct method should lower");
    let method = lowered
        .functions
        .iter()
        .find(|function| function.name == "Lang.greeting")
        .expect("method should lower as a function");

    assert_eq!(
        method.params,
        vec![
            LoweredParam {
                name: "self".to_string(),
                type_: LoweredType::Struct("Lang".to_string()),
            },
            LoweredParam {
                name: "prefix".to_string(),
                type_: basic(BasicType::String),
            },
        ]
    );

    let source = emit_c(&lowered);
    assert!(source.contains("// Gust function: Lang.greeting"));
    assert!(source.contains("gust_self->gust_name"));
    assert!(source.contains("gust_lang, (gust_rt_string){"));
}

#[test]
fn mutable_member_and_extension_receivers_lower_as_hidden_parameters() {
    let result = check_source(
        r#"struct Counter {
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
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("mutable receivers should lower");
    let method = lowered
        .functions
        .iter()
        .find(|function| function.name == "Counter.increment")
        .expect("mutable method should lower");
    assert_eq!(method.params.len(), 1);
    assert_eq!(method.params[0].name, "self");
    let extension = lowered
        .functions
        .iter()
        .find(|function| function.name == "extension Counter.add")
        .expect("mutable extension should lower");
    assert_eq!(extension.params.len(), 2);
    assert_eq!(extension.params[0].name, "self");
    assert_eq!(extension.params[1].name, "amount");

    let source = emit_c(&lowered);
    assert!(source.contains("gust_self->gust_value++"));
    assert!(source.contains("gust_self->gust_value = (gust_self->gust_value + gust_amount)"));
}

#[test]
fn inferred_method_receiver_types_still_enforce_mutable_self() {
    let result = check_source(
        r#"struct Counter {
    value: i32

    static fn new() => Self { value: 0 }

    fn increment(mut self): void {
        self.value++
    }
}

static fn Counter.make() => Self.new()

fn main() {
    let counter = Counter.make()
    counter.increment()
}"#,
    );

    assert!(
        !result.has_errors(),
        "frontend should defer the inferred receiver type, got {:?}",
        result.diagnostics
    );

    let diagnostics =
        lower_program(&result.program).expect_err("immutable inferred receiver must be rejected");
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains(
                "cannot call mutable function `Counter.increment` through immutable binding `counter`; declare it with `let mut counter`"
            )),
        "expected dedicated immutable receiver error, got {diagnostics:?}"
    );
}

#[test]
fn inferred_constructor_return_preserves_argument_mutability() {
    let result = check_source(
        r#"
struct A {
    value: string
}

struct Container {
    value: A

    static fn new(value: A) => Self {
        value: value,
    }
}

fn main() {
    let mut mutableA = A { value: "Hello" }
    let mut validContainer = Container.new(mutableA)
    let immutableA = A { value: "Hello" }
    let mut invalidContainer = Container.new(immutableA)
}
"#,
    );

    assert!(
        !result.has_errors(),
        "frontend should defer the inferred constructor type, got {:?}",
        result.diagnostics
    );

    let diagnostics = lower_program(&result.program)
        .expect_err("immutable constructor argument must not gain mutable capability");
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("immutable value"))
            .count(),
        1,
        "expected only the immutable constructor argument to fail, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "cannot initialize mutable binding `invalidContainer` from an immutable value"
            )),
        "expected invalid constructor binding error, got {diagnostics:?}"
    );
}

#[test]
fn extension_functions_lower_with_local_static_dispatch() {
    let result = check_source(
        r#"struct Greeter {
    name: string

    fn label(): string => "member"
}

fn Greeter.label(): string => "extension"
fn Greeter.greeting(prefix: string) => prefix + self.name
fn string.withSuffix(suffix: string) => self + suffix

fn main() {
    let greeter = Greeter { name: "Gust" }
    io.println(greeter.label())
    io.println(greeter.greeting("Hello, "))
    io.println("Gust".withSuffix("!"))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("extensions should lower");
    assert!(
        lowered
            .functions
            .iter()
            .any(|function| function.name == "extension Greeter.label")
    );
    assert!(
        lowered
            .functions
            .iter()
            .any(|function| function.name == "extension Greeter.greeting")
    );
    assert!(
        lowered
            .functions
            .iter()
            .any(|function| function.name == "extension string.withSuffix")
    );

    let LoweredStatement::Println(LoweredExpr {
        kind: LoweredExprKind::Call { name, .. },
        ..
    }) = &lowered.statements[1]
    else {
        panic!("expected member call");
    };
    assert_eq!(name, "Greeter.label");

    let source = emit_c(&lowered);
    assert!(source.contains("// Gust function: extension Greeter.greeting"));
    assert!(source.contains("// Gust function: extension string.withSuffix"));
}

#[test]
fn static_members_and_extensions_lower_without_receivers() {
    let result = check_source(
        r#"struct Greeter {
    name: string

    static fn new(name: string): Self => Self { name: name }
    static fn label(): string => "member"
}

static fn Greeter.default(): Self => Self.new("Gust")
static fn Greeter.label(): string => "extension"

fn main() {
    let greeter = Greeter.default()
    io.println(Greeter.label())
    io.println(greeter.name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("static functions should lower");
    let constructor = lowered
        .functions
        .iter()
        .find(|function| function.name == "static Greeter.new")
        .expect("static member should lower");
    assert_eq!(constructor.params.len(), 1);
    let extension = lowered
        .functions
        .iter()
        .find(|function| function.name == "static extension Greeter.default")
        .expect("static extension should lower");
    assert!(extension.params.is_empty());

    let LoweredStatement::Println(LoweredExpr {
        kind: LoweredExprKind::Call { name, args, .. },
        ..
    }) = &lowered.statements[1]
    else {
        panic!("expected static member call");
    };
    assert_eq!(name, "static Greeter.label");
    assert!(args.is_empty());

    let source = emit_c(&lowered);
    assert!(source.contains("// Gust function: static Greeter.new"));
    assert!(source.contains("// Gust function: static extension Greeter.default"));
}
