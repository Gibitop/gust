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
    assert!(source.contains("static void* gust_rt_alloc(const gust_rt_type_desc* desc, size_t size)"));
    assert!(source.contains("header = malloc(sizeof(gust_rt_object_header) + payload_size);"));
    assert!(source.contains("unsigned char* data = gust_rt_alloc(&gust_rt_desc_bytes, byte_len == 0 ? 1 : byte_len);"));
}

#[test]
fn string_interpolation_c_output_is_stable() {
    let result = check_source(
        r#"struct Person {
    name: string
}

fn main() {
    let person = Person { name: "Gust" }
    let count = 2
    io.println("Hello, $person.name ${count + 1}! \$literal")
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected string interpolation to validate, got {:?}",
        result.diagnostics
    );
    let lowered = lower_program(&result.program).expect("string interpolation should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("static gust_rt_string gust_rt_string_concat("));
    assert!(source.contains("static gust_rt_string gust_rt_i32_to_string("));
    assert!(source.contains("gust_rt_string_concat(gust_rt_string_concat("));
    assert!(source.contains("gust_person->gust_name"));
    assert!(source.contains("$literal"));
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
        "#include <stdint.h>\n\nstatic uint64_t gust_fn_848019df_answer();\n\n// Gust function: answer\nstatic uint64_t gust_fn_848019df_answer() {\n    return 42;\n}\n\nint main(void) {\n    uint64_t gust_count = gust_fn_848019df_answer();\n    return 0;\n}\n"
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
    assert!(source.contains("gust_rt_return_value = gust_rt_string_concat((gust_rt_string){"));
    assert!(source.contains("gust_rt_roots_pop_to(gust_rt_function_roots);"));
    assert!(source.contains("return gust_rt_return_value;"));
}

#[test]
fn top_level_static_initializers_emit_c_startup_assignments_and_roots() {
    let result = check_source(
        r#"let base = "Hello, "
let message = greet("Gust")

fn greet(name: string): string {
    return base + name
}

fn main() {
    io.println(message)
}"#,
    );
    assert!(
        !result.has_errors(),
        "expected top-level static initializers to validate, got {:?}",
        result.diagnostics
    );
    let lowered = lower_program(&result.program).expect("top-level statics should lower");

    let source = emit_c(&lowered);
    assert!(source.contains("static gust_rt_string gust_base;"));
    assert!(source.contains("static gust_rt_string gust_message;"));
    assert!(source.contains("gust_rt_root_slot gust_rt_root_base"));
    assert!(source.contains("gust_rt_root_slot gust_rt_root_message"));
    assert!(source.contains("gust_base = (gust_rt_string){"));
    assert!(source.contains("gust_message = gust_fn_fb1de34a_greet((gust_rt_string){"));
    assert!(source.contains("gust_rt_io_println(gust_message);"));
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
    assert!(source.contains("gust_rt_return_value = gust_lang->gust_name;"));
    assert!(source.contains("return gust_rt_return_value;"));
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

    assert!(source.contains("static void* gust_rt_alloc(const gust_rt_type_desc* desc, size_t size)"));
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

#[test]
fn panic_emits_runtime_stack_trace_helpers() {
    let source = r#"fn fail() {
    panic("boom")
}

fn main() {
    fail()
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered =
        lower_program_with_source(&result.program, "panic.gust", source).expect("panic should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("static void gust_rt_panic(gust_rt_string message)"));
    assert!(source.contains("fputs(\"panic: \", stderr);"));
    assert!(source.contains("fputs(\"stack trace:\\n\", stderr);"));
    assert!(source.contains("exit(101);"));
    assert!(source.contains("gust_rt_stack_push(\"main\", \"panic.gust\", 5, 1);"));
    assert!(source.contains("gust_rt_stack_push(\"fail\", \"panic.gust\", 1, 1);"));
    assert!(source.contains("gust_rt_stack_update(\"panic.gust\", 2, 5);"));
    assert!(source.contains("gust_rt_panic((gust_rt_string){"));
}

#[test]
fn panic_exits_non_zero_and_prints_stack_trace() {
    let source = r#"fn fail() {
    panic("boom")
}

fn wrapper() {
    fail()
}

fn main() {
    wrapper()
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered =
        lower_program_with_source(&result.program, "panic.gust", source).expect("panic should lower");
    let source = emit_c(&lowered);
    let test_dir = std::env::temp_dir().join(format!("gust-panic-test-{}", std::process::id()));
    fs::create_dir_all(&test_dir).expect("panic test temp dir should be created");
    let c_path = test_dir.join("panic.c");
    let executable = test_dir.join("panic");
    fs::write(&c_path, source).expect("generated C should be written");

    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build panic executable");
    assert!(
        output.status.success(),
        "generated panic C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(&executable)
        .output()
        .expect("panic executable should run");
    assert_eq!(output.status.code(), Some(101));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("panic: boom"), "{stderr}");
    assert!(stderr.contains("stack trace:\n"), "{stderr}");
    assert!(stderr.contains("  at fail (panic.gust:2:5)\n"), "{stderr}");
    assert!(stderr.contains("  at wrapper (panic.gust:6:5)\n"), "{stderr}");
    assert!(stderr.contains("  at main (panic.gust:10:5)\n"), "{stderr}");
    assert!(
        stderr.find("  at fail (panic.gust:2:5)\n")
            < stderr.find("  at wrapper (panic.gust:6:5)\n")
            && stderr.find("  at wrapper (panic.gust:6:5)\n")
                < stderr.find("  at main (panic.gust:10:5)\n"),
        "{stderr}"
    );
}

#[test]
fn panic_rejects_non_string_operands() {
    let result = check_source(
        r#"fn main() {
    panic(1)
}"#,
    );

    assert!(
        result.has_errors(),
        "expected frontend type error for panic, got {:?}",
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
        "expected string panic diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn gc_stress_keeps_registered_roots_alive() {
    let source = r#"struct Box {
    text: string
}

fn make(label: string, value: i32): Box {
    return Box { text: label + value.toString() }
}

fn main() {
    let first = make("first ", 1)
    let second = make("second ", 2)
    io.println(first.text)
    io.println(second.text)
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program_with_source(&result.program, "gc_stress.gust", source)
        .expect("GC stress should lower");
    let source = emit_c_with_options(&lowered, CCodegenOptions { gc_stress: true });
    assert!(source.contains("gust_rt_root_push"));
    assert!(source.contains("gust_rt_mark_roots();"));
    assert!(source.contains("static const bool gust_rt_gc_stress = true;"));

    let test_dir = std::env::temp_dir().join(format!("gust-gc-stress-test-{}", std::process::id()));
    fs::create_dir_all(&test_dir).expect("GC stress temp dir should be created");
    let c_path = test_dir.join("gc_stress.c");
    let executable = test_dir.join("gc_stress");
    fs::write(&c_path, source).expect("generated C should be written");

    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build GC stress executable");
    assert!(
        output.status.success(),
        "generated GC stress C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(&executable)
        .output()
        .expect("GC stress executable should run");
    assert!(
        output.status.success(),
        "GC stress executable should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "first 1\nsecond 2\n"
    );
}

#[test]
fn gc_stress_covers_control_flow_matches_closures_and_trait_objects() {
    let source = r#"struct Box {
    text: string
}

enum Choice {
    Boxed(Box)
    Text(string)
}

trait Describe {
    fn describe(): string
}

impl Describe for Box {
    fn describe(): string {
        return self.text
    }
}

fn choose(useBox: bool, label: string): Choice {
    if useBox {
        let boxed = Box { text: label + " box" }
        return Choice.Boxed(boxed)
    }

    let text = label + " text"
    return Choice.Text(text)
}

fn describeChoice(choice: Choice): string {
    return match choice {
        Choice.Boxed(boxed) => boxed.text,
        Choice.Text(text) => text,
    }
}

fn makeFormatter(prefix: string): fn(i32): string {
    let captured = Box { text: prefix + ":" }
    return fn(value) => captured.text + value.toString()
}

fn printDescription(value: Describe) {
    io.println(value.describe())
}

fn main() {
    let first = choose(true, "first")
    let second = choose(false, "second")
    io.println(describeChoice(first))
    io.println(describeChoice(second))

    let formatter = makeFormatter("count")
    io.println(formatter(7))

    let described = Box { text: "trait " + "object" }
    printDescription(described)

    let mut index = 0
    while index < 5 {
        let item = Box { text: "loop " + index.toString() }
        index++
        if index == 2 {
            continue
        }
        if index == 5 {
            break
        }
        io.println(item.text)
    }
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program_with_source(&result.program, "gc_roots.gust", source)
        .expect("GC root stress should lower");
    let source = emit_c_with_options(&lowered, CCodegenOptions { gc_stress: true });
    assert!(source.contains("static const bool gust_rt_gc_stress = true;"));

    let test_dir =
        std::env::temp_dir().join(format!("gust-gc-roots-test-{}", std::process::id()));
    fs::create_dir_all(&test_dir).expect("GC roots temp dir should be created");
    let c_path = test_dir.join("gc_roots.c");
    let executable = test_dir.join("gc_roots");
    fs::write(&c_path, source).expect("generated C should be written");

    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build GC root stress executable");
    assert!(
        output.status.success(),
        "generated GC root stress C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(&executable)
        .output()
        .expect("GC root stress executable should run");
    assert!(
        output.status.success(),
        "GC root stress executable should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "first box\nsecond text\ncount:7\ntrait object\nloop 0\nloop 2\nloop 3\n"
    );
}
