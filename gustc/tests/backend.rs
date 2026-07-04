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
fn if_else_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    let enabled = true

    if enabled {
        io.println("enabled")
    } else {
        io.println("disabled")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("if/else should lower");

    assert_eq!(
        lowered.statements[1],
        LoweredStatement::If {
            condition: LoweredExpr {
                type_: basic(BasicType::Bool),
                kind: LoweredExprKind::Local("enabled".to_string()),
            },
            then_branch: vec![LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::StringLiteral("enabled".to_string()),
            })],
            else_branch: Some(vec![LoweredStatement::Println(LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::StringLiteral("disabled".to_string()),
            })]),
        }
    );
}

#[test]
fn inferred_returning_if_else_emits_c() {
    let result = check_source(
        r#"fn choose(enabled: bool) {
    if enabled {
        return "enabled"
    } else {
        return "disabled"
    }
}

fn main() {
    io.println(choose(true))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("returning if/else should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("if (gust_enabled) {\n        return \"enabled\";"));
    assert!(source.contains("} else {\n        return \"disabled\";"));
    assert!(source.contains("gust_rt_io_println(gust_fn_"));
    assert!(!source.contains("return ;"));
}

#[test]
fn inferred_recursive_return_type_emits_c() {
    let result = check_source(
        r#"fn fib(n: i32) {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    if fib(10) == 55 {
        io.println("fib works")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("recursive function should lower");
    let fib = lowered
        .functions
        .iter()
        .find(|function| function.name == "fib")
        .expect("fib should be lowered");

    assert_eq!(fib.return_type, basic(BasicType::I32));
    assert_eq!(fib.return_value.type_, basic(BasicType::I32));

    let source = emit_c(&lowered);

    assert!(source.contains("static int32_t gust_fn_"));
    assert!(source.contains("gust_rt_io_println(\"fib works\");"));
}

#[test]
fn unresolved_recursive_return_type_has_a_dedicated_error() {
    let result = check_source(
        r#"fn recurse() {
    return recurse()
}

fn main() {
    recurse()
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let diagnostics =
        lower_program(&result.program).expect_err("unresolved recursion should not lower");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "could not infer return type of function `recurse`; add an explicit return type"
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
fn inferred_arrow_void_and_early_return_helpers_emit_c() {
    let result = check_source(
        r#"fn inferred(name: String) {
    return "Hello, " + name
}

fn arrow(name: String) => inferred(name)

fn noop(): void {}

fn early(): String {
    return arrow("Gust")
    return "unreachable"
}

fn nested() {
    io.println(early())
    noop()
}

fn main() {
    nested()
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("function features should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("static const char* gust_fn_"));
    assert!(source.contains("static void gust_fn_"));
    assert!(source.contains("    return gust_fn_"));
    assert!(source.contains("    gust_rt_io_println(gust_fn_"));
}

#[test]
fn conflicting_inferred_return_types_have_a_dedicated_error() {
    let result = check_source(
        r#"fn inconsistent() {
    return "Gust"
    return true
}

fn main() {
    inconsistent()
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let diagnostics =
        lower_program(&result.program).expect_err("inconsistent returns should not lower");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "function `inconsistent` has multiple return types (`String` and `bool`); inferred return types must be consistent"
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
    assert!(source.contains("struct gust_struct_"));
    assert!(source.contains("_Person {"));
    assert!(source.contains("const char* gust_name;"));
    assert!(source.contains("uint32_t gust_age;"));
    assert!(source.contains("gust_person = gust_rt_new_gust_struct_"));
    assert!(source.contains(".gust_name = \"Gust\""));
    assert!(source.contains(".gust_age = 1"));
    assert!(source.contains("gust_rt_io_println(gust_person->gust_name);"));
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

    assert!(source.contains("typedef struct gust_struct_f1168775_Lang gust_struct_f1168775_Lang;"));
    assert!(source.contains("struct gust_struct_f1168775_Lang {"));
    assert!(source.contains("static gust_struct_f1168775_Lang* gust_fn_de4514cf_makeLang() {"));
    assert!(source.contains(
        "static const char* gust_fn_1f1b2f34_getName(gust_struct_f1168775_Lang* gust_lang) {"
    ));
    assert!(source.contains("return gust_lang->gust_name;"));
    assert!(source.contains("gust_struct_f1168775_Lang* gust_lang = gust_fn_de4514cf_makeLang();"));
    assert!(source.contains("gust_rt_io_println(gust_fn_1f1b2f34_getName(gust_lang));"));
    assert!(source.contains("gust_rt_io_println(gust_fn_de4514cf_makeLang()->gust_name);"));
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
    label: String
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
        "gust_state->gust_label = gust_rt_string_concat(gust_state->gust_label, \" updated\");"
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
    text: String
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
    assert!(source.contains("gust_message = gust_rt_string_concat(gust_message, \" world\");"));
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
    name: String
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
        "gust_person->gust_name = gust_rt_string_concat(gust_person->gust_name, \"!\");"
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

#[test]
fn numeric_comparisons_lower_with_contextual_literals() {
    let result = check_source(
        r#"fn main() {
    let age: u32 = 30

    if 18 <= age {
        io.println("adult")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("numeric comparison should lower");
    let LoweredStatement::If { condition, .. } = &lowered.statements[1] else {
        panic!("expected lowered if statement");
    };
    let LoweredExprKind::Comparison { left, op, right } = &condition.kind else {
        panic!("expected lowered comparison");
    };

    assert_eq!(*op, gustc::ast::BinaryOp::LessEqual);
    assert_eq!(left.type_, basic(BasicType::U32));
    assert_eq!(right.type_, basic(BasicType::U32));
}

#[test]
fn string_comparisons_emit_value_equality_runtime_helper() {
    let result = check_source(
        r#"fn main() {
    let name = "Gu" + "st"

    if name == "Gust" {
        io.println("equal")
    }

    if name != "Rust" {
        io.println("not equal")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("string comparisons should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("static bool gust_rt_string_equal("));
    assert!(source.contains("return strcmp(left, right) == 0;"));
    assert!(source.contains("if (gust_rt_string_equal(gust_name, \"Gust\"))"));
    assert!(source.contains("if (!gust_rt_string_equal(gust_name, \"Rust\"))"));
}

#[test]
fn numeric_comparisons_emit_native_c_operators() {
    let result = check_source(
        r#"fn main() {
    let age: u32 = 30
    let equal = age == 30
    let notEqual = age != 0
    let less = age < 31
    let lessEqual = age <= 30
    let greater = age > 29
    let greaterEqual = age >= 30
}"#,
    );
    let lowered = lower_program(&result.program).expect("numeric comparisons should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("(gust_age == 30)"));
    assert!(source.contains("(gust_age != 0)"));
    assert!(source.contains("(gust_age < 31)"));
    assert!(source.contains("(gust_age <= 30)"));
    assert!(source.contains("(gust_age > 29)"));
    assert!(source.contains("(gust_age >= 30)"));
}

#[test]
fn math_operators_lower_and_emit_native_c_with_precedence() {
    let result = check_source(
        r#"fn main() {
    let value: i32 = -2 + 3 * 4 - 8 / 2 % 3
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("math operators should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("(((-2) + (3 * 4)) - ((8 / 2) % 3))"));
}

#[test]
fn numeric_add_preserves_annotated_integer_type() {
    let result = check_source(
        r#"fn main() {
    let value: u64 = 1 + 2
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("numeric add should lower");

    assert_eq!(
        lowered.statements[0],
        LoweredStatement::Local {
            name: "value".to_string(),
            value: LoweredExpr {
                type_: basic(BasicType::U64),
                kind: LoweredExprKind::Arithmetic {
                    left: Box::new(LoweredExpr {
                        type_: basic(BasicType::U64),
                        kind: LoweredExprKind::NumberLiteral("1".to_string()),
                    }),
                    op: gustc::ast::BinaryOp::Add,
                    right: Box::new(LoweredExpr {
                        type_: basic(BasicType::U64),
                        kind: LoweredExprKind::NumberLiteral("2".to_string()),
                    }),
                },
            },
        }
    );
}

#[test]
fn logical_operators_lower_and_emit_native_c() {
    let result = check_source(
        r#"fn main() {
    let age: u32 = 30
    let enabled = true
    let disabled = false

    if age >= 18 && enabled && !disabled {
        io.println("access granted")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("logical operators should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("&&"));
    assert!(source.contains("(!gust_disabled)"));
    assert!(source.contains("(gust_age >= 18)"));
}

#[test]
fn logical_operators_preserve_short_circuiting_in_c() {
    let result = check_source(
        r#"fn shouldNotRun(): bool {
    io.println("unexpected")
    return true
}

fn main() {
    if false && shouldNotRun() {}
    if true || shouldNotRun() {
        io.println("done")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("logical operators should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("(false && gust_fn_"));
    assert!(source.contains("(true || gust_fn_"));
}

#[test]
fn payload_enums_and_matches_emit_tagged_union_c() {
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
fn struct_enum_fields_emit_after_their_enum_definition() {
    let result = check_source(
        r#"struct Spaceship {
    pilot: Being
}

enum Being {
    Person(String)
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
