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
fn while_break_and_continue_lower_and_emit_c() {
    let result = check_source(
        r#"fn main() {
    let mut index = 0

    while index < 5 {
        index += 1

        if index == 2 {
            continue
        }

        if index == 4 {
            break
        }

        io.println(index.toString())
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("while should lower");

    assert!(
        matches!(lowered.statements[1], LoweredStatement::While { .. }),
        "expected second statement to be lowered while, got {:?}",
        lowered.statements
    );

    let source = emit_c(&lowered);

    assert!(source.contains("while ("));
    assert!(source.contains("continue;\n"));
    assert!(source.contains("break;\n"));
    assert!(source.contains("gust_rt_io_println(gust_rt_i32_to_string(gust_index));"));
}

#[test]
fn bare_break_match_branch_lowers_and_emits_c() {
    let result = check_source(
        r#"enum Step {
    More(i32)
    Done
}

fn next(value: i32): Step {
    if value < 3 {
        return Step.More(value)
    }
    return Step.Done
}

fn main() {
    let mut value = 0

    while true {
        value++
        match next(value) {
            Step.More(current) => io.println(current.toString()),
            Step.Done => break
        }
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("bare break match branch should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("while (true)"));
    assert!(source.contains("break;\n"));
}

#[test]
fn iterator_for_loop_lowers_and_emits_c() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

trait Iterator<T> {
    fn next(mut self): Option<T>
}

struct Counter {
    value: i32
    end: i32
}

impl Iterator<i32> for Counter {
    fn next(mut self): Option<i32> {
        if self.value < self.end {
            let value = self.value
            self.value++
            return Option.Some(value)
        }

        return Option<i32>.None
    }
}

fn main() {
    let mut counter = Counter { value: 1, end: 5 }

    for value in counter {
        if value == 2 {
            continue
        }

        if value == 4 {
            break
        }

        io.println(value.toString())
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("iterator for loop should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("while (true)"));
    assert!(source.contains("gust_method_next"));
    assert!(source.contains("continue;\n"));
    assert!(source.contains("break;\n"));
}

#[test]
fn iterable_for_loop_lowers_and_emits_c() {
    let result = check_source(
        r#"enum Option<T> {
    Some(T)
    None
}

trait Iterator<T> {
    fn next(mut self): Option<T>
}

trait Iterable<T> {
    fn iterator(): Iterator<T>
}

struct Counter {
    start: i32
    end: i32
}

struct CounterIterator {
    value: i32
    end: i32
}

impl Iterator<i32> for CounterIterator {
    fn next(mut self): Option<i32> {
        if self.value < self.end {
            let value = self.value
            self.value++
            return Option.Some(value)
        }

        return Option<i32>.None
    }
}

impl Iterable<i32> for Counter {
    fn iterator(): Iterator<i32> => CounterIterator {
        value: self.start,
        end: self.end
    }
}

fn main() {
    let counter = Counter { start: 1, end: 3 }

    for value in counter {
        io.println(value.toString())
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("iterable for loop should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("while (true)"));
    assert!(source.contains("gust_method_next"));
    assert!(source.contains("break;\n"));
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
fn inferred_return_function_values_lower_and_emit_c() {
    let result = check_source(
        r#"fn apply(value: i32, f: fn(i32): i32) {
    return f(value)
}

fn addOne(value: i32) {
    return value + 1
}

fn main() {
    if apply(41, addOne) == 42 {
        io.println("function value works")
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("function value should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("function value works"));
    assert!(source.contains(".gust_call("));
}

#[test]
fn inferred_closure_return_with_void_body_and_runtime_helpers_emit_c() {
    let result = check_source(
        r#"fn makeCallCounter() {
    let mut count = 0

    return fn () {
        count++
        io.println(count.toString())
    }
}

fn main() {
    let callCounter = makeCallCounter()
    callCounter()
    callCounter()
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("closure return should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("static const char* gust_rt_i32_to_string("));
    assert!(source.contains("gust_rt_io_println(gust_rt_i32_to_string("));
    assert!(source.contains(".gust_call("));
}

#[test]
fn incompatible_inferred_return_function_values_are_lowering_errors() {
    let result = check_source(
        r#"fn useString(f: fn(i32): String): String {
    return f(1)
}

fn addOne(value: i32) {
    return value + 1
}

fn main() {
    io.println(useString(addOne))
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected frontend to defer inferred return mismatch, got {:?}",
        result.diagnostics
    );

    let diagnostics =
        lower_program(&result.program).expect_err("inferred function return should mismatch");

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected value of type `fn(i32): String`, got `fn(i32): i32`")),
        "expected inferred function return mismatch, got {diagnostics:?}"
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
            raw_buffer_element: None,
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
    assert!(source.contains("const char* gust_name;"));
    assert!(source.contains("uint32_t gust_age;"));
    assert!(source.contains("const char* gust_name, uint32_t gust_age)"));
    assert!(source.contains("result->gust_name = gust_name;"));
    assert!(source.contains("result->gust_age = gust_age;"));
    assert!(source.contains("gust_person = gust_rt_new_gust_struct_"));
    assert!(source.contains("_Person(\"Gust\", 1);"));
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
        r#"fn i32.toString(): String => "extension"

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
}

#[test]
fn struct_methods_lower_to_functions_with_self_receivers() {
    let result = check_source(
        r#"struct Lang {
    name: String

    fn greeting(prefix: String) {
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
    assert!(source.contains("gust_lang, \"Hello, \""));
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
    value: String
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
    name: String

    fn label(): String => "member"
}

fn Greeter.label(): String => "extension"
fn Greeter.greeting(prefix: String) => prefix + self.name
fn String.withSuffix(suffix: String) => self + suffix

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
            .any(|function| function.name == "extension String.withSuffix")
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
    assert!(source.contains("// Gust function: extension String.withSuffix"));
}

#[test]
fn static_members_and_extensions_lower_without_receivers() {
    let result = check_source(
        r#"struct Greeter {
    name: String

    static fn new(name: String): Self => Self { name: name }
    static fn label(): String => "member"
}

static fn Greeter.default(): Self => Self.new("Gust")
static fn Greeter.label(): String => "extension"

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
        kind: LoweredExprKind::Call { name, args },
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

#[test]
fn computed_block_matches_and_string_patterns_emit_c() {
    let result = check_source(
        r#"enum Being {
    Person(String)
    Unknown
}

fn constructBeing(kind: String): Being {
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
fn mutable_enum_payload_patterns_lower_to_payload_access() {
    let result = check_source(
        r#"struct StringContainer {
    value: String

    fn set(mut self, value: String) {
        self.value = value
    }
}

enum Option {
    Some(StringContainer)
    None

    fn set(mut self, value: String) {
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
    Person(String)
    Unknown
}

fn constructBeing(kind: String): Being {
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

    assert!(source.contains("const char* gust_internal_match_value_"));
    assert!(source.contains("_result;"));
    assert!(source.contains("_result = \"Hello\";"));
    assert!(source.contains("_result = \"Hi\";"));
    assert!(source.contains("gust_rt_io_println(gust_rt_string_concat("));
}

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

    assert!(source.contains("// Gust struct: Box<String>"));
    assert!(source.contains("// Gust struct: Box<bool>"));
    assert!(source.contains("// Gust struct: Box<i32>"));
    assert!(source.contains("// Gust function: Box<String>.get"));
    assert!(source.contains("// Gust function: Box<i32>.get"));
    assert!(source.contains("// Gust function: static Box<i32>.new"));
    assert!(source.contains("// Gust function: static Box<i32>.build"));
    assert!(source.contains("// Gust function: Box<String>.getValue"));
    assert!(source.contains("// Gust function: Box<i32>.getValue"));
    assert!(!source.contains("// Gust function: static Box<String>.new"));
    assert!(!source.contains("// Gust function: static Box<bool>.new"));
    assert!(!source.contains("// Gust function: static Box<String>.build"));
    assert!(!source.contains("// Gust function: Box<bool>.get"));
    assert!(!source.contains("// Gust function: Box<bool>.getValue"));
    assert!(!source.contains(".addOne"));
    assert!(!source.contains(".unused"));
    assert!(!source.contains("// Gust function: Box<String>.replace"));
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

fn optionText(value: Option<String>): String {
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
    let text = Option<String>.Some("Gust")
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

    assert!(source.contains("// Gust enum: Option<String>"));
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
    assert!(c.contains("identity<String>"));
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

fn describe(value: Option<String>): String {
    return match value {
        Option.Some(inner) => inner,
        Option.None => "empty",
    }
}

fn main() {
    let number = Box { value: 42 }
    let pair = number.pair("answer")
    let staticPair = Box<i32>.make<String>(7, "static")
    let empty: Option<String> = number.empty()
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
    assert!(c.contains("// Gust function: Box<i32>.pair<String>"));
    assert!(c.contains("// Gust function: static Box<i32>.make<String>"));
    assert!(c.contains("// Gust function: Box<i32>.empty<String>"));
    assert!(c.contains("// Gust struct: Pair<i32, String>"));
    assert!(c.contains("// Gust enum: Option<String>"));
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

    let lowered = lower_program(&result.program).expect("trait impl should lower");
    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: trait Person.describe"));
    assert!(c.contains("// Gust function: trait Person.update"));
    assert!(c.contains("// Gust function: static trait Person.new"));
    assert!(c.contains("gust_fn_"));
}

#[test]
fn trait_typed_values_lower_to_dynamic_dispatch() {
    let result = check_source(
        r#"impl Describe for Person {
    fn describe() => self.name
}

trait Describe {
    fn describe(): String
}

struct Person {
    name: String
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
    fn describe(): String {
        return match self {
            Mood.Happy => "happy",
            Mood.Sad => "sad",
        }
    }
}

trait Describe {
    fn describe(): String
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
    assert!(c.contains("gust_trait_self = gust_rt_alloc(sizeof(gust_enum_"));
    assert!(c.contains("*(("));
    assert!(c.contains("*)gust_self)"));
    assert!(c.contains(".gust_vtable = &gust_vtable_"));
    assert!(c.contains(".gust_method_describe"));
}

#[test]
fn generic_trait_typed_values_lower_to_dynamic_dispatch() {
    let result = check_source(
        r#"impl Named<String> for Person {
    fn name() => self.name
}

trait Named<T> {
    fn name(): T
}

struct Person {
    name: String
}

fn main() {
    let person = Person { name: "Gust" }
    let named: Named<String> = person
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
            } if matches!(&value.kind, LoweredExprKind::TraitObject { trait_name, .. } if trait_name == "Named<String>")
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
            ) && object.type_.name() == "Named<String>"
        ),
        "expected generic trait method call to lower as dynamic call, got {:?}",
        lowered.statements
    );

    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: trait Named<String> for Person.name"));
    assert!(c.contains("gust_trait_thunk_"));
    assert!(c.contains("Named_String"));
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
    let value = Box<String> { value: "Gust" }
    let named: Named<String> = value
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
            } if matches!(&value.kind, LoweredExprKind::TraitObject { trait_name, .. } if trait_name == "Named<String>")
        ),
        "expected generic trait impl template local to lower as trait object, got {:?}",
        lowered.statements
    );

    let c = emit_c(&lowered);
    assert!(c.contains("// Gust function: trait Named<String> for Box<String>.name"));
    assert!(c.contains("gust_trait_thunk_"));
    assert!(c.contains("Named_String"));
}

#[test]
fn into_impls_lower_to_target_specific_trait_calls() {
    let source = include_str!("../../examples/into.gust");
    let result = check_source(source);
    assert!(
        !result.has_errors(),
        "expected Into conversions to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("Into conversions should lower");
    let c = emit_c(&lowered);

    assert!(c.contains("// Gust function: trait Into<UserId> for String.into"));
    assert!(c.contains("// Gust function: trait Into<Label> for String.into"));
    assert!(c.contains("// Gust function: static trait From<String> for UserId.from"));
    assert!(c.contains("// Gust function: static trait From<String> for Label.from"));
    assert!(c.contains("gust_fn_"));
}
