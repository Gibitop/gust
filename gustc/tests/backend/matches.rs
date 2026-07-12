#[test]
fn payload_enums_and_matches_emit_tagged_union_c() {
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
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("enums and matches should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("// Gust enum: Being"));
    assert!(source.contains("gust_enum_"));
    assert!(source.contains("gust_tag"));
    assert!(source.contains("gust_payload"));
    assert!(source.contains(".gust_tag =="));
}

#[test]
fn nested_enum_payload_patterns_emit_nested_tag_checks() {
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

fn main() {
    io.println(label(Option.Some(Result.Ok("ready"))))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("nested enum matches should lower");
    let source = emit_c(&lowered);

    assert!(source.contains(".gust_payload.gust_Some"));
    assert!(source.contains("_payload_"));
    assert!(source.contains(".gust_tag =="));
}

#[test]
fn or_patterns_emit_or_conditions_and_bind_matching_payloads() {
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
        Option.Some(Result.Ok(text)) | Option.Some(Result.Err(text)) => text,
        Option.None => "none",
    }
}

fn main() {
    io.println(label(Option.Some(Result.Ok("ready"))))
    io.println(label(Option.Some(Result.Err("waiting"))))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("or-patterns should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("bool gust_internal_match_or_"));
    assert!(source.contains(" = true;"));
    assert!(source.contains(".gust_payload.gust_Ok"));
    assert!(source.contains(".gust_payload.gust_Err"));
}

#[test]
fn struct_patterns_lower_to_field_access_replacements() {
    let result = check_source(
        r#"struct Person {
    name: string
    age: i32
}

enum MaybePerson {
    Some(Person)
    None
}

fn personName(person: Person): string {
    return match person {
        Person { name, ... } => name,
    }
}

fn maybeName(value: MaybePerson): string {
    return match value {
        MaybePerson.Some(Person { name: personName, ... }) => personName,
        MaybePerson.None => "none",
    }
}

fn main() {
    let person = Person { name: "Ada", age: 37 }
    io.println(personName(person))
    io.println(maybeName(MaybePerson.Some(person)))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("struct patterns should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("->gust_name"));
    assert!(source.contains(".gust_payload.gust_Some"));
}

#[test]
fn match_guards_lower_to_combined_conditions() {
    let result = check_source(
        r#"struct Person {
    name: string
    age: i32
}

enum MaybePerson {
    Some(Person)
    None
}

fn personName(person: Person): string {
    return match person {
        Person { name, age } if age >= 18 => name,
        Person { name, ... } => name,
    }
}

fn maybeName(value: MaybePerson): string {
    return match value {
        MaybePerson.Some(Person { name, age }) if age >= 18 => name,
        MaybePerson.Some(Person { name, ... }) => name,
        MaybePerson.None => "none",
    }
}

fn main() {
    let person = Person { name: "Ada", age: 37 }
    io.println(personName(person))
    io.println(maybeName(MaybePerson.Some(person)))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("match guards should lower");
    let source = emit_c(&lowered);

    assert!(source.contains(">= 18"));
    assert!(source.contains("->gust_age"));
    assert!(source.contains(".gust_payload.gust_Some"));
}

#[test]
fn struct_enum_fields_emit_after_their_enum_definition() {
    let result = check_source(
        r#"struct Spaceship {
    pilot: Being
}

enum Being {
    Person(string)
    Unknown
}

fn main() {
    let spaceship = Spaceship {
        pilot: Being.Person("Ada"),
    }
    let name = match spaceship.pilot {
        Being.Person(name) => name,
        Being.Unknown => "Unknown pilot",
    }
    io.println(name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("enum struct fields should lower");
    let source = emit_c(&lowered);
    let enum_position = source.find("// Gust enum: Being").expect("enum definition");
    let struct_position = source
        .find("// Gust struct: Spaceship")
        .expect("struct definition");

    assert!(enum_position < struct_position);
}

#[test]
fn computed_block_matches_and_string_patterns_emit_c() {
    let result = check_source(
        r#"enum Being {
    Person(string)
    Unknown
}

fn constructBeing(kind: string): Being {
    return match kind {
        "person" => Being.Person("Ada"),
        _ => Being.Unknown,
    }
}

fn main() {
    let mut name = ""
    match constructBeing("person") {
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
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("new match forms should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("gust_rt_string_equal(gust_internal_match_value_"));
    assert!(source.contains(".gust_tag =="));
    assert_eq!(
        source
            .lines()
            .filter(|line| {
                line.contains("gust_internal_match_value_") && line.contains("= gust_fn_")
            })
            .count(),
        1
    );
}

#[test]
fn integer_and_bool_literal_patterns_emit_typed_c_conditions() {
    let result = check_source(
        r#"fn codeLabel(code: u64): string {
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

fn main() {
    io.println(codeLabel(200))
    io.println(wideLabel(340282366920938463463374607431768211455))
    io.println(flagLabel(true))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("literal patterns should lower");
    let source = emit_c(&lowered);

    assert!(source.contains(" == 200"));
    assert!(source.contains(" >= 400"));
    assert!(source.contains(" <= 499"));
    assert!(source.contains(" == true"));
    assert!(source.contains("(unsigned __int128)"));
    assert!(!source.contains("340282366920938463463374607431768211455"));
}

#[test]
fn mutable_enum_payload_patterns_lower_to_payload_access() {
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
    match option {
        Option.Some(container) => io.println(container.value),
        Option.None => io.println("None"),
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected mutable payload pattern to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("mutable payload pattern should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("// Gust function: StringContainer.set"));
    assert!(source.contains(".gust_payload.gust_Some"));
}

#[test]
fn block_bodied_match_expression_branches_emit_c() {
    let result = check_source(
        r#"enum Being {
    Person(string)
    Unknown
}

fn constructBeing(kind: string): Being {
    if kind == "person" {
        return Being.Person("Ada")
    }
    return Being.Unknown
}

fn main() {
    let mut name = ""
    let greeting = match constructBeing("person") {
        Being.Person(personName) => {
            let extractedName = personName
            name = extractedName
            return "Hello"
        },
        Being.Unknown => {
            name = "stranger"
            return "Hi"
        },
    }
    io.println(greeting + ", " + name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("block match expression should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("gust_rt_string gust_internal_match_value_"));
    assert!(source.contains("_result;"));
    assert!(source.contains("_result = (gust_rt_string){"));
    assert!(source.contains("gust_rt_io_println(gust_rt_string_concat("));
}
