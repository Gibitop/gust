#[test]
fn hello_world_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    io.println("Hello, world!")
}"#,
    );
    let lowered = lower_program(&result.program).expect("hello world should lower");

    let source = emit_c(&lowered);
    assert!(source.contains("typedef struct {\n    const unsigned char* gust_data;"));
    assert!(source.contains(".gust_byte_len = 13"));
    assert!(source.contains("fwrite(value.gust_data, 1, value.gust_byte_len, stdout);"));
}

#[test]
fn string_local_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let message = "Hello, string local!"
    io.println(message)
}"#,
    );
    let lowered = lower_program(&result.program).expect("string local should lower");

    let source = emit_c(&lowered);
    assert!(source.contains("gust_rt_string gust_message"));
    assert!(source.contains(".gust_byte_len = 20"));
    assert!(source.contains("gust_rt_io_println(gust_message);"));
}

#[test]
fn string_concat_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let name = "Gust"
    let message = "Hello, " + name + "!"
    io.println("Inline " + "concat")
    io.println(message)
}"#,
    );
    let lowered = lower_program(&result.program).expect("string concat should lower");
    let source = emit_c(&lowered);

    assert!(source.contains(
        "static gust_rt_string gust_rt_string_concat(gust_rt_string left, gust_rt_string right)"
    ));
    assert!(source.contains("size_t byte_len = left.gust_byte_len + right.gust_byte_len;"));
    assert!(source.contains(".gust_byte_len = 4"));
    assert_eq!(source.matches("malloc(").count(), 1);
    assert!(source.contains("return malloc(size);"));
    assert!(source.contains("unsigned char* data = gust_rt_alloc(byte_len == 0 ? 1 : byte_len);"));
}

#[test]
fn numeric_helper_call_c_output_is_stable() {
    let result = check_source(
        r#"fn answer(): u64 {
    return 42
}

fn main() {
    let count = answer()
}"#,
    );
    let lowered = lower_program(&result.program).expect("numeric helper should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdint.h>\n\n// Gust function: answer\nstatic uint64_t gust_fn_848019df_answer() {\n    return 42;\n}\n\nint main(void) {\n    uint64_t gust_count = gust_fn_848019df_answer();\n    return 0;\n}\n"
    );
}

#[test]
fn string_helper_call_c_output_is_stable() {
    let result = check_source(
        r#"fn greet(name: string): string {
    return "Hello, " + name
}

fn main() {
    io.println(greet("Gust") + "!")
}"#,
    );
    let lowered = lower_program(&result.program).expect("string helper should lower");

    let source = emit_c(&lowered);
    assert!(
        source.contains("static gust_rt_string gust_fn_fb1de34a_greet(gust_rt_string gust_name)")
    );
    assert!(source.contains("return gust_rt_string_concat((gust_rt_string){"));
}

#[test]
fn basic_struct_c_output_contains_typedef_literal_and_field_access() {
    let result = check_source(
        r#"struct Person {
    name: string
    age: u32
}

fn main() {
    let person = Person {
        age: 1,
        name: "Gust",
    }
    io.println(person.name)
}"#,
    );
    let lowered = lower_program(&result.program).expect("basic struct should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("// Gust struct: Person"));
    assert!(source.contains("typedef struct gust_struct_"));
    assert!(source.contains("struct gust_struct_"));
    assert!(source.contains("_Person {"));
    assert!(source.contains("gust_rt_string gust_name;"));
    assert!(source.contains("uint32_t gust_age;"));
    assert!(source.contains("gust_rt_string gust_name, uint32_t gust_age)"));
    assert!(source.contains("result->gust_name = gust_name;"));
    assert!(source.contains("result->gust_age = gust_age;"));
    assert!(source.contains("gust_person = gust_rt_new_gust_struct_"));
    assert!(source.contains("_Person((gust_rt_string){"));
    assert!(source.contains("gust_rt_io_println(gust_person->gust_name);"));
}

#[test]
fn empty_structs_emit_c_padding_without_gust_fields() {
    let result = check_source(
        r#"struct Token {
}

fn main() {
    let token = Token {}
    let copied = token.clone()
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected empty struct to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("empty struct should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("// Gust struct: Token"));
    assert!(source.contains("bool gust_empty;"));
    assert!(source.contains("gust_rt_new_"));
}

#[test]
fn struct_helper_values_c_output_contains_struct_signatures() {
    let result = check_source(
        r#"struct Lang {
    name: string
    version: u32
}

fn makeLang(): Lang {
    return Lang {
        name: "Gust",
        version: 1,
    }
}

fn getName(lang: Lang): string {
    return lang.name
}

fn main() {
    let lang = makeLang()
    io.println(getName(lang))
    io.println(makeLang().name)
}"#,
    );
    let lowered = lower_program(&result.program).expect("struct helper values should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("typedef struct gust_struct_f1168775_Lang gust_struct_f1168775_Lang;"));
    assert!(source.contains("struct gust_struct_f1168775_Lang {"));
    assert!(source.contains("static gust_struct_f1168775_Lang* gust_fn_de4514cf_makeLang() {"));
    assert!(source.contains(
        "static gust_rt_string gust_fn_1f1b2f34_getName(gust_struct_f1168775_Lang* gust_lang) {"
    ));
    assert!(source.contains("return gust_lang->gust_name;"));
    assert!(source.contains("gust_struct_f1168775_Lang* gust_lang = gust_fn_de4514cf_makeLang();"));
    assert!(source.contains("gust_rt_io_println(gust_fn_1f1b2f34_getName(gust_lang));"));
    assert!(source.contains("gust_rt_io_println(gust_fn_de4514cf_makeLang()->gust_name);"));
}

#[test]
fn user_function_named_alloc_does_not_collide_with_runtime_alloc() {
    let result = check_source(
        r#"fn alloc(name: string): string {
    return "Hello, " + name
}

fn main() {
    io.println(alloc("Gust"))
}"#,
    );
    let lowered = lower_program(&result.program).expect("alloc helper should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("static void* gust_rt_alloc(size_t size)"));
    assert!(source.contains("// Gust function: alloc"));
    assert!(source.contains("static gust_rt_string gust_fn_bab1bb16_alloc("));
    assert!(source.contains("gust_rt_io_println(gust_fn_bab1bb16_alloc((gust_rt_string){"));
}

#[test]
fn basic_local_defaults_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let message: string
    let count: i32
    let flag: bool
    let byte: u8
    let size: usize
}"#,
    );
    let lowered = lower_program(&result.program).expect("basic defaults should lower");

    let source = emit_c(&lowered);
    assert!(source.contains("gust_rt_string gust_message = (gust_rt_string){ .gust_data = (const unsigned char*)\"\", .gust_byte_len = 0 };"));
}

#[test]
fn initialized_basic_locals_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let message = "Hello, initialized!"
    let count: u64 = 42
    let flag = true
}"#,
    );
    let lowered = lower_program(&result.program).expect("initialized basics should lower");

    let source = emit_c(&lowered);
    assert!(source.contains("gust_rt_string gust_message = (gust_rt_string){"));
    assert!(source.contains(".gust_byte_len = 19"));
}

#[test]
fn float_and_128_bit_numeric_types_lower_and_emit_c() {
    let result = check_source(
        r#"fn main() {
    let signed: i128 = 170141183460469231731687303715884105727
    let minimum: i128 = -170141183460469231731687303715884105728
    let unsigned: u128 = 340282366920938463463374607431768211455
    let single: f32 = 1 / 2
    let double = 5.5 % 2
}"#,
    );
    let lowered = lower_program(&result.program).expect("extended numeric types should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("__int128 gust_signed"));
    assert!(source.contains("__int128 gust_minimum = ((__int128)(-"));
    assert!(source.contains("unsigned __int128 gust_unsigned"));
    assert!(source.contains("float gust_single = (1.0f / 2.0f);"));
    assert!(source.contains("double gust_double = fmod(5.5, 2.0);"));
    assert!(source.contains("#include <math.h>"));
    assert!(!source.contains("340282366920938463463374607431768211455"));
}

#[test]
fn numeric_to_string_lowers_and_emits_type_specific_runtime_helpers() {
    let result = check_source(
        r#"fn main() {
    let u8Number: u8 = 1
    let u16Number: u16 = 2
    let u32Number: u32 = 3
    let u64Number: u64 = 4
    let u128Number: u128 = 5
    let usizeNumber: usize = 6
    let i8Number: i8 = -7
    let i16Number: i16 = -8
    let i32Number: i32 = -9
    let i64Number: i64 = -10
    let i128Number: i128 = -11
    let f32Number: f32 = 1.25
    let f64Number: f64 = 2.5

    io.println(u8Number.toString())
    io.println(u16Number.toString())
    io.println(u32Number.toString())
    io.println(u64Number.toString())
    io.println(u128Number.toString())
    io.println(usizeNumber.toString())
    io.println(i8Number.toString())
    io.println(i16Number.toString())
    io.println(i32Number.toString())
    io.println(i64Number.toString())
    io.println(i128Number.toString())
    io.println(f32Number.toString())
    io.println(f64Number.toString())
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("numeric toString calls should lower");
    let source = emit_c(&lowered);

    for type_name in [
        "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "f32", "f64",
    ] {
        assert!(
            source.contains(&format!("gust_rt_{type_name}_to_string")),
            "expected runtime helper for {type_name} in:\n{source}"
        );
    }

    assert!(source.contains("snprintf(NULL, 0, \"%.9g\", (double)value)"));
    assert!(source.contains("snprintf(NULL, 0, \"%.17g\", value)"));
    assert!(source.contains("(unsigned __int128)(-(value + 1)) + 1"));
    assert!(source.contains("gust_rt_io_println(gust_rt_i32_to_string(gust_i32Number));"));
}

#[test]
fn numeric_to_string_is_lowered_as_an_intrinsic_expression() {
    let result = check_source(
        r#"fn i32.toString(): string => "extension"

fn main() {
    let number: i32 = 42
    let text = number.toString()
}"#,
    );
    let lowered = lower_program(&result.program).expect("numeric toString should lower");

    assert_eq!(
        lowered.statements[1],
        LoweredStatement::Local {
            name: "text".to_string(),
            value: LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::NumberToString(Box::new(LoweredExpr {
                    type_: basic(BasicType::I32),
                    kind: LoweredExprKind::Local("number".to_string()),
                })),
            },
        }
    );
}

#[test]
fn c_output_mangles_local_names_that_are_c_keywords() {
    let result = check_source(
        r#"fn main() {
    let short: u16 = 16
    let unsigned: u32 = 32
    let signed = 32
}"#,
    );
    let lowered = lower_program(&result.program).expect("keyword-like locals should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdint.h>\n\nint main(void) {\n    uint16_t gust_short = 16;\n    uint32_t gust_unsigned = 32;\n    int32_t gust_signed = 32;\n    return 0;\n}\n"
    );
}

#[test]
fn c_output_escapes_string_values() {
    let result = check_source(
        r#"fn main() {
    io.println("line\n\"quote\"\\slash")
}"#,
    );
    let lowered = lower_program(&result.program).expect("escaped string should lower");

    let source = emit_c(&lowered);
    assert!(source.contains("line\\n\\\"quote\\\"\\\\slash"));
    assert!(source.contains(".gust_byte_len = 18"));
}

#[test]
fn strings_preserve_embedded_nul_bytes() {
    let result = check_source(
        r#"fn main() {
    let value = "left\0right"
    if value == "left\0right" {
        io.println(value + "!")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("embedded NUL string should lower");
    let source = emit_c(&lowered);

    assert!(source.contains(".gust_byte_len = 10"));
    assert!(source.contains("memcmp(left.gust_data, right.gust_data, left.gust_byte_len) == 0;"));
    assert!(!source.contains("strlen("));
    assert!(!source.contains("strcmp("));
}

#[test]
fn println_rejects_non_string_operands() {
    let result = check_source(
        r#"fn main() {
    let count = 1
    io.println(count)
    let flag = true
    io.println(flag)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let diagnostics = lower_program(&result.program).expect_err("source should not lower");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("only accepts `string` values")
                && diagnostic.message.contains("`i32`")),
        "expected numeric println diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("only accepts `string` values")
                && diagnostic.message.contains("`bool`")),
        "expected bool println diagnostic, got {diagnostics:?}"
    );
}

