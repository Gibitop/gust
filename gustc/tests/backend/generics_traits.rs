#[test]
fn generic_struct_specializations_emit_distinct_c_types_and_methods() {
    let result = check_source(
        r#"struct Box<T> {
    value: T

    static fn new(value: T) => Self.build(value)

    static fn build(value: T) => Self { value: value }

    static fn unused(value: T): T => value + 1

    fn get() {
        return self.getValue()
    }

    fn getValue() {
        return self.value
    }

    fn replace(mut self, value: T) {
        self.value = value
    }

    fn addOne(): T {
        return self.value + 1
    }
}

fn main() {
    let mut number = Box { value: 42 }
    let constructed = Box.new(7)
    let text = Box { value: "Generics work!" }
    let flag = Box { value: true }
    number.replace(43)
    io.println(number.get().toString())
    io.println(constructed.get().toString())
    io.println(text.get())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic structs should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("// Gust struct: Box<string>"));
    assert!(source.contains("// Gust struct: Box<bool>"));
    assert!(source.contains("// Gust struct: Box<i32>"));
    assert!(source.contains("// Gust function: Box<string>.get"));
    assert!(source.contains("// Gust function: Box<i32>.get"));
    assert!(source.contains("// Gust function: static Box<i32>.new"));
    assert!(source.contains("// Gust function: static Box<i32>.build"));
    assert!(source.contains("// Gust function: Box<string>.getValue"));
    assert!(source.contains("// Gust function: Box<i32>.getValue"));
    assert!(!source.contains("// Gust function: static Box<string>.new"));
    assert!(!source.contains("// Gust function: static Box<bool>.new"));
    assert!(!source.contains("// Gust function: static Box<string>.build"));
    assert!(!source.contains("// Gust function: Box<bool>.get"));
    assert!(!source.contains("// Gust function: Box<bool>.getValue"));
    assert!(!source.contains(".addOne"));
    assert!(!source.contains(".unused"));
    assert!(!source.contains("// Gust function: Box<string>.replace"));
}

#[test]
fn generic_enum_specializations_emit_distinct_c_types_and_match_payloads() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

enum Wrapper<T> {
    Value(T)
}

fn optionText(value: Option<string>): string {
    return match value {
        Option.Some(inner) => inner,
        Option.None => "missing",
    }
}

fn nestedNumber(value: Wrapper<Option<i32>>): i32 {
    return match value {
        Wrapper.Value(option) => match option {
            Option.Some(inner) => inner,
            Option.None => 0,
        },
    }
}

fn main() {
    let number = Option.Some(42)
    let text = Option<string>.Some("Gust")
    let nested = Wrapper.Value(number)
    io.println(optionText(text))
    io.println(nestedNumber(nested).toString())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic enums should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("// Gust enum: Option<string>"));
    assert!(source.contains("// Gust enum: Option<i32>"));
    assert!(source.contains("// Gust enum: Wrapper<Option<i32>>"));
    assert!(source.contains(".gust_payload."));
    assert!(source.contains(".gust_tag =="));
}

#[test]
fn generic_function_specializations_emit_distinct_c_functions() {
    let result = check_source(
        r#"fn identity<T>(value: T) => value

fn main() {
    let number = identity(42)
    let text = identity("Gust")
    io.println(number.toString())
    io.println(text)
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected generic functions to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic functions should lower");
    let c = emit_c(&lowered);
    assert!(c.contains("identity<i32>"));
    assert!(c.contains("identity<string>"));
}

#[test]
fn generic_closures_specialize_function_types_and_capture_layouts() {
    let result = check_source(
        r#"fn makeIdentity<T>(): fn(T): T => fn(value) => value

fn makeStored<T>(value: T): fn(): T {
    let stored = value
    return fn() => stored
}

fn identity<T>(value: T): T => value

fn apply<T>(value: T, transform: fn(T): T): T => transform(value)

fn passThrough<T, U>(transform: fn(T): U): fn(T): U => transform

fn main() {
    let numberIdentity: fn(i32): i32 = makeIdentity<i32>()
    let textIdentity: fn(string): string = makeIdentity<string>()
    let result = apply(41, identity)
    let numberValue = makeStored(7)
    let textValue = makeStored("closure")
    let stringify: fn(i32): string = passThrough(fn(value) => value.toString())

    io.println(numberIdentity(42).toString())
    io.println(textIdentity("gust"))
    io.println(result.toString())
    io.println(numberValue().toString())
    io.println(textValue())
    io.println(stringify(9))
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected generic closures to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic closures should lower");
    assert!(lowered.closure_functions.iter().any(|function| {
        function.params.len() == 1
            && function.params[0].type_ == basic(BasicType::I32)
            && function.return_type == basic(BasicType::I32)
    }));
    assert!(lowered.closure_functions.iter().any(|function| {
        function.params.len() == 1
            && function.params[0].type_ == basic(BasicType::String)
            && function.return_type == basic(BasicType::String)
    }));
    assert!(lowered.closure_functions.iter().any(|function| {
        function.captures.len() == 1
            && function.captures[0].name == "stored"
            && function.captures[0].type_ == basic(BasicType::I32)
    }));
    assert!(lowered.closure_functions.iter().any(|function| {
        function.captures.len() == 1
            && function.captures[0].name == "stored"
            && function.captures[0].type_ == basic(BasicType::String)
    }));

    let c = emit_c(&lowered);
    assert!(c.contains("int32_t (*gust_call)(void*, int32_t)"));
    assert!(c.contains("gust_rt_string (*gust_call)(void*, gust_rt_string)"));
    assert!(c.contains("int32_t* gust_stored"));
    assert!(c.contains("gust_rt_string* gust_stored"));
    assert!(c.contains("passThrough<i32, string>"));
}

#[test]
fn generic_method_specializations_emit_distinct_c_methods() {
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

    fn empty<U>() => Option<U>.None

    fn unused<U>(value: U): U => value
}

fn describe(value: Option<string>): string {
    return match value {
        Option.Some(inner) => inner,
        Option.None => "empty",
    }
}

fn main() {
    let number = Box { value: 42 }
    let pair = number.pair("answer")
    let staticPair = Box<i32>.make<string>(7, "static")
    let empty: Option<string> = number.empty()
    io.println(pair.second)
    io.println(staticPair.second)
    io.println(describe(empty))
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected generic methods to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic methods should lower");
    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: Box<i32>.pair<string>"));
    assert!(c.contains("// Gust function: static Box<i32>.make<string>"));
    assert!(c.contains("// Gust function: Box<i32>.empty<string>"));
    assert!(c.contains("// Gust struct: Pair<i32, string>"));
    assert!(c.contains("// Gust enum: Option<string>"));
    assert!(!c.contains(".unused"));
}

#[test]
fn generic_enum_methods_lower_with_self_receivers() {
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
    io.println(present.unwrapOr(0).toString())
    io.println(absent.unwrapOr(7).toString())
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected generic enum methods to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic enum methods should lower");
    let method = lowered
        .functions
        .iter()
        .find(|function| function.name == "Option<i32>.unwrapOr")
        .expect("enum method should lower as a function");
    assert_eq!(
        method.params,
        vec![
            LoweredParam {
                name: "self".to_string(),
                type_: LoweredType::Enum("Option<i32>".to_string()),
            },
            LoweredParam {
                name: "fallback".to_string(),
                type_: basic(BasicType::I32),
            },
        ]
    );

    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: Option<i32>.unwrapOr"));
    assert!(c.contains(".gust_tag =="));
}

#[test]
fn trait_impl_methods_lower_to_static_calls() {
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

    let lowered = lower_program(&result.program).expect("trait impl should lower");
    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: trait Person.describe"));
    assert!(c.contains("// Gust function: trait Person.update"));
    assert!(c.contains("// Gust function: static trait Person.new"));
    assert!(c.contains("gust_fn_"));
}

#[test]
fn associated_types_lower_through_generic_impls_and_projections() {
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

    let lowered = lower_program(&result.program).expect("associated types should lower");
    let c = emit_c(&lowered);
    assert!(c.contains("trait Index<usize, type Output: i32> for Box<i32>.index"));
    assert!(c.contains("static int32_t"));
}

#[test]
fn bound_associated_trait_objects_emit_resolved_vtables() {
    let result = check_source(
        r#"trait Producer {
    type Item
    fn next(): Self.Item
}

struct Counter {
    value: i32
}

impl Producer for Counter {
    type Item: i32
    fn next(): i32 => self.value
}

fn main() {
    let producer: Producer<type Item: i32> = Counter { value: 7 }
    io.println(producer.next().toString())
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected associated trait object to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("associated trait object should lower");
    let c = emit_c(&lowered);
    assert!(c.contains("Producer_type_Item__i32"));
    assert!(c.contains("gust_method_next"));
    assert!(c.contains("gust_vtable"));
}

#[test]
fn trait_typed_values_lower_to_dynamic_dispatch() {
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

fn main() {
    let person = Person { name: "Gust" }
    let described: Describe = person
    io.println(described.describe())
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected trait object program to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("trait object should lower");

    assert!(
        matches!(
            lowered.statements[1],
            LoweredStatement::Local {
                ref value,
                ..
            } if matches!(value.kind, LoweredExprKind::TraitObject { .. })
        ),
        "expected trait-typed local to lower as trait object, got {:?}",
        lowered.statements
    );
    assert!(
        matches!(
            lowered.statements[2],
            LoweredStatement::Println(LoweredExpr {
                kind: LoweredExprKind::DynamicCall { .. },
                ..
            })
        ),
        "expected trait method call to lower as dynamic call, got {:?}",
        lowered.statements
    );

    let c = emit_c(&lowered);
    assert!(c.contains("gust_vtable_"));
    assert!(c.contains("gust_trait_thunk_"));
    assert!(c.contains(".gust_vtable = &gust_vtable_"));
    assert!(c.contains(".gust_method_describe"));
}

#[test]
fn enum_trait_typed_values_lower_to_dynamic_dispatch() {
    let result = check_source(
        r#"impl Describe for Mood {
    fn describe(): string {
        return match self {
            Mood.Happy => "happy",
            Mood.Sad => "sad",
        }
    }
}

trait Describe {
    fn describe(): string
}

enum Mood {
    Happy
    Sad
}

fn printDescription(value: Describe) {
    io.println(value.describe())
}

fn current(): Describe {
    return Mood.Happy
}

fn main() {
    let described: Describe = Mood.Happy
    io.println(described.describe())
    printDescription(Mood.Sad)
    io.println(current().describe())
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected enum trait object program to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("enum trait object should lower");

    assert!(
        matches!(
            lowered.statements[0],
            LoweredStatement::Local {
                ref value,
                ..
            } if matches!(
                &value.kind,
                LoweredExprKind::TraitObject {
                    self_type: LoweredType::Enum(name),
                    ..
                } if name == "Mood"
            )
        ),
        "expected enum trait-typed local to lower as trait object, got {:?}",
        lowered.statements
    );
    assert!(
        matches!(
            lowered.statements[1],
            LoweredStatement::Println(LoweredExpr {
                kind: LoweredExprKind::DynamicCall { .. },
                ..
            })
        ),
        "expected enum trait method call to lower as dynamic call, got {:?}",
        lowered.statements
    );
    assert!(
        matches!(
            lowered.statements[2],
            LoweredStatement::Expr(LoweredExpr {
                kind: LoweredExprKind::Call { ref args, .. },
                ..
            }) if matches!(
                args.first().map(|arg| &arg.kind),
                Some(LoweredExprKind::TraitObject {
                    self_type: LoweredType::Enum(name),
                    ..
                }) if name == "Mood"
            )
        ),
        "expected enum trait-typed argument to lower as trait object, got {:?}",
        lowered.statements
    );
    assert!(
        lowered.functions.iter().any(|function| {
            function.name == "current"
                && matches!(
                    &function.return_value.kind,
                    LoweredExprKind::TraitObject {
                        self_type: LoweredType::Enum(name),
                        ..
                    } if name == "Mood"
                )
        }),
        "expected enum trait-typed return to lower as trait object, got {:?}",
        lowered.functions
    );

    let c = emit_c(&lowered);
    assert!(c.contains("gust_trait_self = gust_rt_alloc(&gust_rt_desc_enum_"));
    assert!(c.contains("*(("));
    assert!(c.contains("*)gust_self)"));
    assert!(c.contains(".gust_vtable = &gust_vtable_"));
    assert!(c.contains(".gust_method_describe"));
}

#[test]
fn generic_trait_typed_values_lower_to_dynamic_dispatch() {
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

fn main() {
    let person = Person { name: "Gust" }
    let named: Named<string> = person
    io.println(named.name())
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected generic trait object program to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic trait object should lower");

    assert!(
        matches!(
            lowered.statements[1],
            LoweredStatement::Local {
                ref value,
                ..
            } if matches!(&value.kind, LoweredExprKind::TraitObject { trait_name, .. } if trait_name == "Named<string>")
        ),
        "expected generic trait-typed local to lower as trait object, got {:?}",
        lowered.statements
    );
    assert!(
        matches!(
            lowered.statements[2],
            LoweredStatement::Println(LoweredExpr {
                kind: LoweredExprKind::DynamicCall { ref object, .. },
                ..
            }) if matches!(
                object.kind,
                LoweredExprKind::Local(ref name) if name == "named"
            ) && object.type_.name() == "Named<string>"
        ),
        "expected generic trait method call to lower as dynamic call, got {:?}",
        lowered.statements
    );

    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: trait Named<string> for Person.name"));
    assert!(c.contains("gust_trait_thunk_"));
    assert!(c.contains("Named_string"));
}

#[test]
fn generic_trait_impl_templates_lower_to_dynamic_dispatch() {
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

fn main() {
    let value = Box<string> { value: "Gust" }
    let named: Named<string> = value
    io.println(named.name())
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected generic trait impl template program to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("generic trait impl template should lower");

    assert!(
        matches!(
            lowered.statements[1],
            LoweredStatement::Local {
                ref value,
                ..
            } if matches!(&value.kind, LoweredExprKind::TraitObject { trait_name, .. } if trait_name == "Named<string>")
        ),
        "expected generic trait impl template local to lower as trait object, got {:?}",
        lowered.statements
    );

    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: trait Named<string> for Box<string>.name"));
    assert!(c.contains("gust_trait_thunk_"));
    assert!(c.contains("Named_string"));
}

#[test]
fn into_impls_lower_to_target_specific_trait_calls() {
    let source = include_str!("../../../examples/into.gust");
    let result = check_source(source);
    assert!(
        !result.has_errors(),
        "expected Into conversions to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("Into conversions should lower");
    let c = emit_c(&lowered);

    assert!(c.contains("// Gust function: trait Into<UserId> for string.into"));
    assert!(c.contains("// Gust function: trait Into<Label> for string.into"));
    assert!(c.contains("// Gust function: static trait From<string> for UserId.from"));
    assert!(c.contains("// Gust function: static trait From<string> for Label.from"));
    assert!(c.contains("gust_fn_"));
}

#[test]
fn generic_associated_type_projections_lower_through_concrete_impls() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

trait Mapper {
    type Wrapped<T>
    fn value(): Self.Wrapped<i32>
}

struct Numbers {}

impl Mapper for Numbers {
    type Wrapped<T>: Option<T>
    fn value(): Option<i32> => Option.Some(7)
}

fn read<M: Mapper>(mapper: M): M.Wrapped<i32> => mapper.value()

fn main() {
    let numbers = Numbers {}
    match read(numbers) {
        Option.Some(value) => io.println(value.toString())
        Option.None => io.println("missing")
    }
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected generic associated type projection to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program)
        .expect("generic associated type projection should lower");
    let c = emit_c(&lowered);
    assert!(c.contains("trait Numbers.value"));
    assert!(c.contains("Option<i32>"));
}
