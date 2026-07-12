#[test]
fn struct_field_access_lowers_successfully() {
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
    let name: string = person.name
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

