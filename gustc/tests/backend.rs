use gustc::ast::BasicType;
use gustc::c_codegen::emit_c;
use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lower::{
    LoweredExpr, LoweredExprKind, LoweredField, LoweredFunction, LoweredParam, LoweredStatement,
    LoweredStruct, LoweredStructFieldValue, LoweredType, lower_program,
};

fn basic(type_: BasicType) -> LoweredType {
    LoweredType::Basic(type_)
}

#[test]
fn hello_world_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    io.println("Hello, world!")
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("hello world should lower");

    assert_eq!(
        lowered.statements,
        vec![LoweredStatement::Println(LoweredExpr {
            type_: basic(BasicType::String),
            kind: LoweredExprKind::StringLiteral("Hello, world!".to_string()),
        })]
    );
}

#[test]
fn string_local_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    let message = "Hello, string local!"
    io.println(message)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("string local should lower");

    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::Local {
                name: "message".to_string(),
                value: LoweredExpr {
                    type_: basic(BasicType::String),
                    kind: LoweredExprKind::StringLiteral("Hello, string local!".to_string()),
                },
            },
            LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::Local("message".to_string()),
            }),
        ]
    );
}

#[test]
fn string_concat_local_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    let name = "Gust"
    let message = "Hello, " + name
    io.println(message)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("string concat local should lower");
    let LoweredStatement::Local { value, .. } = &lowered.statements[1] else {
        panic!("expected string concat local");
    };

    assert_eq!(value.type_, basic(BasicType::String));

    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::Local {
                name: "name".to_string(),
                value: LoweredExpr {
                    type_: basic(BasicType::String),
                    kind: LoweredExprKind::StringLiteral("Gust".to_string()),
                },
            },
            LoweredStatement::Local {
                name: "message".to_string(),
                value: LoweredExpr {
                    type_: basic(BasicType::String),
                    kind: LoweredExprKind::StringConcat(
                        Box::new(LoweredExpr {
                            type_: basic(BasicType::String),
                            kind: LoweredExprKind::StringLiteral("Hello, ".to_string()),
                        }),
                        Box::new(LoweredExpr {
                            type_: basic(BasicType::String),
                            kind: LoweredExprKind::Local("name".to_string()),
                        }),
                    ),
                },
            },
            LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::Local("message".to_string()),
            }),
        ]
    );
}

#[test]
fn direct_string_concat_println_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    let name = "Gust"
    io.println("Hello, " + name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("direct string concat should lower");

    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::Local {
                name: "name".to_string(),
                value: LoweredExpr {
                    type_: basic(BasicType::String),
                    kind: LoweredExprKind::StringLiteral("Gust".to_string()),
                },
            },
            LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::StringConcat(
                    Box::new(LoweredExpr {
                        type_: basic(BasicType::String),
                        kind: LoweredExprKind::StringLiteral("Hello, ".to_string()),
                    }),
                    Box::new(LoweredExpr {
                        type_: basic(BasicType::String),
                        kind: LoweredExprKind::Local("name".to_string()),
                    }),
                ),
            }),
        ]
    );
}

#[test]
fn string_returning_helper_lowers_successfully() {
    let result = check_source(
        r#"fn greet(name: String): String {
    return "Hello, " + name
}

fn main() {
    let message = greet("Gust")
    io.println(message)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("function call should lower");

    assert_eq!(
        lowered.functions,
        vec![LoweredFunction {
            name: "greet".to_string(),
            params: vec![LoweredParam {
                name: "name".to_string(),
                type_: basic(BasicType::String),
            }],
            return_type: basic(BasicType::String),
            statements: vec![],
            return_value: LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::StringConcat(
                    Box::new(LoweredExpr {
                        type_: basic(BasicType::String),
                        kind: LoweredExprKind::StringLiteral("Hello, ".to_string()),
                    }),
                    Box::new(LoweredExpr {
                        type_: basic(BasicType::String),
                        kind: LoweredExprKind::Local("name".to_string()),
                    }),
                ),
            },
        }]
    );
    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::Local {
                name: "message".to_string(),
                value: LoweredExpr {
                    type_: basic(BasicType::String),
                    kind: LoweredExprKind::Call {
                        name: "greet".to_string(),
                        args: vec![LoweredExpr {
                            type_: basic(BasicType::String),
                            kind: LoweredExprKind::StringLiteral("Gust".to_string()),
                        }],
                    },
                },
            },
            LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::Local("message".to_string()),
            }),
        ]
    );
}

#[test]
fn basic_struct_local_lowers_successfully() {
    let result = check_source(
        r#"struct Person {
    name: String
    age: u32
}

fn main() {
    let person = Person {
        name: "Gust",
        age: 1,
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("basic struct should lower");

    assert_eq!(
        lowered.structs,
        vec![LoweredStruct {
            name: "Person".to_string(),
            fields: vec![
                LoweredField {
                    name: "name".to_string(),
                    type_: basic(BasicType::String),
                },
                LoweredField {
                    name: "age".to_string(),
                    type_: basic(BasicType::U32),
                },
            ],
        }]
    );
    assert_eq!(
        lowered.statements,
        vec![LoweredStatement::Local {
            name: "person".to_string(),
            value: LoweredExpr {
                type_: LoweredType::Struct("Person".to_string()),
                kind: LoweredExprKind::StructLiteral {
                    name: "Person".to_string(),
                    fields: vec![
                        LoweredStructFieldValue {
                            name: "name".to_string(),
                            value: LoweredExpr {
                                type_: basic(BasicType::String),
                                kind: LoweredExprKind::StringLiteral("Gust".to_string()),
                            },
                        },
                        LoweredStructFieldValue {
                            name: "age".to_string(),
                            value: LoweredExpr {
                                type_: basic(BasicType::U32),
                                kind: LoweredExprKind::NumberLiteral("1".to_string()),
                            },
                        },
                    ],
                },
            },
        }]
    );
}

#[test]
fn struct_field_access_lowers_successfully() {
    let result = check_source(
        r#"struct Person {
    name: String
    age: u32
}

fn main() {
    let person = Person {
        name: "Gust",
        age: 1,
    }
    let name: String = person.name
    io.println(person.name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("field access should lower");

    assert_eq!(
        lowered.statements[1],
        LoweredStatement::Local {
            name: "name".to_string(),
            value: LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::FieldAccess {
                    object: Box::new(LoweredExpr {
                        type_: LoweredType::Struct("Person".to_string()),
                        kind: LoweredExprKind::Local("person".to_string()),
                    }),
                    field: "name".to_string(),
                },
            },
        }
    );
    assert_eq!(
        lowered.statements[2],
        LoweredStatement::Println(LoweredExpr {
            type_: basic(BasicType::String),
            kind: LoweredExprKind::FieldAccess {
                object: Box::new(LoweredExpr {
                    type_: LoweredType::Struct("Person".to_string()),
                    kind: LoweredExprKind::Local("person".to_string()),
                }),
                field: "name".to_string(),
            },
        })
    );
}

#[test]
fn struct_helper_values_lower_successfully() {
    let result = check_source(
        r#"struct Lang {
    name: String
    version: u32
}

fn makeLang(): Lang {
    return Lang {
        name: "Gust",
        version: 1,
    }
}

fn getName(lang: Lang): String {
    return lang.name
}

fn main() {
    let lang = makeLang()
    io.println(getName(lang))
    io.println(makeLang().name)
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("struct helper values should lower");
    let lang_type = LoweredType::Struct("Lang".to_string());

    assert_eq!(
        lowered.functions[0],
        LoweredFunction {
            name: "makeLang".to_string(),
            params: vec![],
            return_type: lang_type.clone(),
            statements: vec![],
            return_value: LoweredExpr {
                type_: lang_type.clone(),
                kind: LoweredExprKind::StructLiteral {
                    name: "Lang".to_string(),
                    fields: vec![
                        LoweredStructFieldValue {
                            name: "name".to_string(),
                            value: LoweredExpr {
                                type_: basic(BasicType::String),
                                kind: LoweredExprKind::StringLiteral("Gust".to_string()),
                            },
                        },
                        LoweredStructFieldValue {
                            name: "version".to_string(),
                            value: LoweredExpr {
                                type_: basic(BasicType::U32),
                                kind: LoweredExprKind::NumberLiteral("1".to_string()),
                            },
                        },
                    ],
                },
            },
        }
    );
    assert_eq!(
        lowered.functions[1],
        LoweredFunction {
            name: "getName".to_string(),
            params: vec![LoweredParam {
                name: "lang".to_string(),
                type_: lang_type.clone(),
            }],
            return_type: basic(BasicType::String),
            statements: vec![],
            return_value: LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::FieldAccess {
                    object: Box::new(LoweredExpr {
                        type_: lang_type.clone(),
                        kind: LoweredExprKind::Local("lang".to_string()),
                    }),
                    field: "name".to_string(),
                },
            },
        }
    );
    assert_eq!(
        lowered.statements,
        vec![
            LoweredStatement::Local {
                name: "lang".to_string(),
                value: LoweredExpr {
                    type_: lang_type.clone(),
                    kind: LoweredExprKind::Call {
                        name: "makeLang".to_string(),
                        args: vec![],
                    },
                },
            },
            LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::Call {
                    name: "getName".to_string(),
                    args: vec![LoweredExpr {
                        type_: lang_type.clone(),
                        kind: LoweredExprKind::Local("lang".to_string()),
                    }],
                },
            }),
            LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::FieldAccess {
                    object: Box::new(LoweredExpr {
                        type_: lang_type,
                        kind: LoweredExprKind::Call {
                            name: "makeLang".to_string(),
                            args: vec![],
                        },
                    }),
                    field: "name".to_string(),
                },
            }),
        ]
    );
}

#[test]
fn hello_world_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    io.println("Hello, world!")
}"#,
    );
    let lowered = lower_program(&result.program).expect("hello world should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n\nstatic void gust_rt_io_println(const char* value) {\n    puts(value);\n}\n\nint main(void) {\n    gust_rt_io_println(\"Hello, world!\");\n    return 0;\n}\n"
    );
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

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n\nstatic void gust_rt_io_println(const char* value) {\n    puts(value);\n}\n\nint main(void) {\n    const char* gust_message = \"Hello, string local!\";\n    gust_rt_io_println(gust_message);\n    return 0;\n}\n"
    );
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

    assert_eq!(
        source,
        "#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n\nstatic void* gust_rt_alloc(size_t size) {\n    return malloc(size);\n}\n\nstatic char* gust_rt_string_concat(const char* left, const char* right) {\n    size_t left_len = strlen(left);\n    size_t right_len = strlen(right);\n    char* result = gust_rt_alloc(left_len + right_len + 1);\n    memcpy(result, left, left_len);\n    memcpy(result + left_len, right, right_len + 1);\n    return result;\n}\n\nstatic void gust_rt_io_println(const char* value) {\n    puts(value);\n}\n\nint main(void) {\n    const char* gust_name = \"Gust\";\n    const char* gust_message = gust_rt_string_concat(gust_rt_string_concat(\"Hello, \", gust_name), \"!\");\n    gust_rt_io_println(gust_rt_string_concat(\"Inline \", \"concat\"));\n    gust_rt_io_println(gust_message);\n    return 0;\n}\n"
    );
    assert_eq!(source.matches("malloc(").count(), 1);
    assert!(source.contains("return malloc(size);"));
    assert!(source.contains("char* result = gust_rt_alloc(left_len + right_len + 1);"));
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
        r#"fn greet(name: String): String {
    return "Hello, " + name
}

fn main() {
    io.println(greet("Gust") + "!")
}"#,
    );
    let lowered = lower_program(&result.program).expect("string helper should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n\nstatic void* gust_rt_alloc(size_t size) {\n    return malloc(size);\n}\n\nstatic char* gust_rt_string_concat(const char* left, const char* right) {\n    size_t left_len = strlen(left);\n    size_t right_len = strlen(right);\n    char* result = gust_rt_alloc(left_len + right_len + 1);\n    memcpy(result, left, left_len);\n    memcpy(result + left_len, right, right_len + 1);\n    return result;\n}\n\nstatic void gust_rt_io_println(const char* value) {\n    puts(value);\n}\n\n// Gust function: greet\nstatic const char* gust_fn_fb1de34a_greet(const char* gust_name) {\n    return gust_rt_string_concat(\"Hello, \", gust_name);\n}\n\nint main(void) {\n    gust_rt_io_println(gust_rt_string_concat(gust_fn_fb1de34a_greet(\"Gust\"), \"!\"));\n    return 0;\n}\n"
    );
}

#[test]
fn basic_struct_c_output_contains_typedef_literal_and_field_access() {
    let result = check_source(
        r#"struct Person {
    name: String
    age: u32
}

fn main() {
    let person = Person {
        name: "Gust",
        age: 1,
    }
    io.println(person.name)
}"#,
    );
    let lowered = lower_program(&result.program).expect("basic struct should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("// Gust struct: Person"));
    assert!(source.contains("typedef struct gust_struct_"));
    assert!(source.contains("_Person {"));
    assert!(source.contains("const char* gust_name;"));
    assert!(source.contains("uint32_t gust_age;"));
    assert!(source.contains("gust_person = (gust_struct_"));
    assert!(source.contains(".gust_name = \"Gust\""));
    assert!(source.contains(".gust_age = 1"));
    assert!(source.contains("gust_rt_io_println(gust_person.gust_name);"));
}

#[test]
fn struct_helper_values_c_output_contains_struct_signatures() {
    let result = check_source(
        r#"struct Lang {
    name: String
    version: u32
}

fn makeLang(): Lang {
    return Lang {
        name: "Gust",
        version: 1,
    }
}

fn getName(lang: Lang): String {
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

    assert!(source.contains("typedef struct gust_struct_f1168775_Lang {"));
    assert!(source.contains("} gust_struct_f1168775_Lang;"));
    assert!(source.contains("static gust_struct_f1168775_Lang gust_fn_de4514cf_makeLang() {"));
    assert!(source.contains(
        "static const char* gust_fn_1f1b2f34_getName(gust_struct_f1168775_Lang gust_lang) {"
    ));
    assert!(source.contains("return gust_lang.gust_name;"));
    assert!(source.contains("gust_struct_f1168775_Lang gust_lang = gust_fn_de4514cf_makeLang();"));
    assert!(source.contains("gust_rt_io_println(gust_fn_1f1b2f34_getName(gust_lang));"));
    assert!(source.contains("gust_rt_io_println(gust_fn_de4514cf_makeLang().gust_name);"));
}

#[test]
fn user_function_named_alloc_does_not_collide_with_runtime_alloc() {
    let result = check_source(
        r#"fn alloc(name: String): String {
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
    assert!(source.contains("static const char* gust_fn_bab1bb16_alloc("));
    assert!(source.contains("gust_rt_io_println(gust_fn_bab1bb16_alloc(\"Gust\"));"));
}

#[test]
fn basic_local_defaults_c_output_is_stable() {
    let result = check_source(
        r#"fn main() {
    let message: String
    let count: i32
    let flag: bool
    let byte: u8
    let size: usize
}"#,
    );
    let lowered = lower_program(&result.program).expect("basic defaults should lower");

    assert_eq!(
        emit_c(&lowered),
        "#include <stdbool.h>\n#include <stddef.h>\n#include <stdint.h>\n\nint main(void) {\n    const char* gust_message = \"\";\n    int32_t gust_count = 0;\n    bool gust_flag = false;\n    uint8_t gust_byte = 0;\n    size_t gust_size = 0;\n    return 0;\n}\n"
    );
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

    assert_eq!(
        emit_c(&lowered),
        "#include <stdbool.h>\n#include <stdint.h>\n\nint main(void) {\n    const char* gust_message = \"Hello, initialized!\";\n    uint64_t gust_count = 42;\n    bool gust_flag = true;\n    return 0;\n}\n"
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

    assert_eq!(
        emit_c(&lowered),
        "#include <stdio.h>\n\nstatic void gust_rt_io_println(const char* value) {\n    puts(value);\n}\n\nint main(void) {\n    gust_rt_io_println(\"line\\n\\\"quote\\\"\\\\slash\");\n    return 0;\n}\n"
    );
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
                && diagnostic.message.contains("only accepts `String` values")
                && diagnostic.message.contains("`i32`")),
        "expected numeric println diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("only accepts `String` values")
                && diagnostic.message.contains("`bool`")),
        "expected bool println diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn mutable_local_is_still_rejected_by_backend() {
    let result = check_source(
        r#"fn main() {
    let mut message = "Gust"
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
                && diagnostic
                    .message
                    .contains("`let mut` bindings are not supported")),
        "expected mutable local diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn mutable_struct_helper_signature_is_rejected_by_backend() {
    let result = check_source(
        r#"
struct Person {
    name: String
}

fn identity(mut person: Person): Person {
    return person
}

fn main() {
}
"#,
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
                && diagnostic
                    .message
                    .contains("mutable parameters are not supported")),
        "expected mutable struct parameter diagnostic, got {diagnostics:?}"
    );
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
    let source = include_str!("../../examples/milestone.gust");
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
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("methods are not supported in executable builds")),
        "expected unsupported-method diagnostic, got {diagnostics:?}"
    );
}
