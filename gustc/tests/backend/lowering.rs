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
fn block_expression_lowers_to_value_expression() {
    let result = check_source(
        r#"fn main() {
    let value = {
        let base = 40
        return base + 2
    }

    io.println(value.toString())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected block expression to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("block expression should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("int32_t gust_base = 40;"));
    assert!(source.contains("gust_base + 2"));
    assert!(!source.contains("// Gust closure: lambda"));
}

#[test]
fn scoped_block_statement_lowers_to_c_block() {
    let result = check_source(
        r#"fn main() {
    {
        let value = 42
        io.println(value.toString())
    }
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected scoped block statement to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("scoped block should lower");

    assert!(
        matches!(lowered.statements[0], LoweredStatement::Block(_)),
        "expected lowered scoped block, got {:?}",
        lowered.statements
    );
}

#[test]
fn block_expression_returned_closure_captures_block_local() {
    let result = check_source(
        r#"fn main() {
    let counter = {
        let mut n = 0

        return fn(): i32 {
            n++
            return n
        }
    }

    io.println(counter().toString())
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected block expression closure to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("block expression closure should lower");

    assert!(
        lowered
            .closure_functions
            .iter()
            .any(|function| function.captures.iter().any(|capture| capture.name == "n")),
        "expected returned closure to capture block local, got {:?}",
        lowered.closure_functions
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

trait Iterator {
    type Item
    fn next(mut self): Option<Self.Item>
}

struct Counter {
    value: i32
    end: i32
}

impl Iterator for Counter {
    type Item: i32
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

trait Iterator {
    type Item
    fn next(mut self): Option<Self.Item>
}

trait Iterable {
    type Item
    fn iterator(): Iterator<type Item: Self.Item>
}

struct Counter {
    start: i32
    end: i32
}

struct CounterIterator {
    value: i32
    end: i32
}

impl Iterator for CounterIterator {
    type Item: i32
    fn next(mut self): Option<i32> {
        if self.value < self.end {
            let value = self.value
            self.value++
            return Option.Some(value)
        }

        return Option<i32>.None
    }
}

impl Iterable for Counter {
    type Item: i32
    fn iterator(): Iterator<type Item: i32> => CounterIterator {
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

    assert!(source.contains("if (gust_enabled) {\n        return (gust_rt_string){"));
    assert!(source.contains("} else {\n        return (gust_rt_string){"));
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
    assert!(source.contains("gust_rt_io_println((gust_rt_string){"));
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
        r#"fn greet(name: string): string {
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
            location: source_location(1),
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
                        location: source_location(1),
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

    assert!(source.contains("static gust_rt_string gust_rt_i32_to_string("));
    assert!(source.contains("gust_rt_io_println(gust_rt_i32_to_string("));
    assert!(source.contains(".gust_call("));
}

#[test]
fn incompatible_inferred_return_function_values_are_lowering_errors() {
    let result = check_source(
        r#"fn useString(f: fn(i32): string): string {
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
            .contains("expected value of type `fn(i32): string`, got `fn(i32): i32`")),
        "expected inferred function return mismatch, got {diagnostics:?}"
    );
}

#[test]
fn inferred_arrow_void_and_early_return_helpers_emit_c() {
    let result = check_source(
        r#"fn inferred(name: string) {
    return "Hello, " + name
}

fn arrow(name: string) => inferred(name)

fn noop(): void {}

fn early(): string {
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

    assert!(source.contains("static gust_rt_string gust_fn_"));
    assert!(source.contains("static void gust_fn_"));
    assert!(source.contains("gust_rt_return_value = gust_fn_"));
    assert!(source.contains("return gust_rt_return_value;"));
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
        "function `inconsistent` has multiple return types (`string` and `bool`); inferred return types must be consistent"
    );
}

#[test]
fn panic_lowers_successfully() {
    let result = check_source(
        r#"fn main() {
    panic("boom")
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected no frontend errors, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("panic should lower");

    assert_eq!(
        lowered.statements,
        vec![LoweredStatement::Panic {
            message: LoweredExpr {
                type_: basic(BasicType::String),
                kind: LoweredExprKind::StringLiteral("boom".to_string()),
            },
            location: source_location(1),
        }]
    );
}

#[test]
fn basic_struct_local_lowers_successfully() {
    let result = check_source(
        r#"struct Person {
    name: string
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
