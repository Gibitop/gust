fn compile_match_branches(
    branches: Vec<CompiledMatchBranch>,
    subject: &str,
    subject_type: &LoweredType,
    enums: &HashMap<String, LoweredEnum>,
    structs: &HashMap<String, LoweredStruct>,
    temp_counter: &mut usize,
) -> LoweredMatchDecision {
    let arms = branches
        .into_iter()
        .map(|branch| {
            let body = LoweredMatchDecision::Body {
                statements: branch.statements,
                value: branch.value,
            };
            let success = if let Some(guard) = branch.guard {
                LoweredMatchDecision::Test {
                    subject: String::new(),
                    test: LoweredMatchTest::Guard(Box::new(guard)),
                    then: Box::new(body),
                    else_: Box::new(LoweredMatchDecision::Fail),
                }
            } else {
                body
            };
            compile_pattern(
                &branch.pattern,
                subject,
                subject_type,
                success,
                LoweredMatchDecision::Fail,
                enums,
                structs,
                temp_counter,
                &HashSet::new(),
            )
        })
        .collect();
    LoweredMatchDecision::Arms { arms }
}

struct CompiledMatchBranch {
    pattern: LoweredPattern,
    guard: Option<LoweredExpr>,
    statements: Vec<LoweredStatement>,
    value: Option<LoweredExpr>,
}

fn compile_pattern(
    pattern: &LoweredPattern,
    subject: &str,
    subject_type: &LoweredType,
    then: LoweredMatchDecision,
    else_: LoweredMatchDecision,
    enums: &HashMap<String, LoweredEnum>,
    structs: &HashMap<String, LoweredStruct>,
    temp_counter: &mut usize,
    assigned_bindings: &HashSet<String>,
) -> LoweredMatchDecision {
    match pattern {
        LoweredPattern::Wildcard => then,
        LoweredPattern::Binding { name } => LoweredMatchDecision::Bind {
            name: name.clone(),
            type_: subject_type.clone(),
            source: LoweredMatchBindSource::Subject(subject.to_string()),
            declare: !assigned_bindings.contains(name),
            then: Box::new(then),
        },
        LoweredPattern::String(value) => LoweredMatchDecision::Test {
            subject: subject.to_string(),
            test: LoweredMatchTest::StringEq(value.clone()),
            then: Box::new(then),
            else_: Box::new(else_),
        },
        LoweredPattern::Bool(value) => LoweredMatchDecision::Test {
            subject: subject.to_string(),
            test: LoweredMatchTest::BoolEq(*value),
            then: Box::new(then),
            else_: Box::new(else_),
        },
        LoweredPattern::Number { value, type_ } => LoweredMatchDecision::Test {
            subject: subject.to_string(),
            test: LoweredMatchTest::NumberEq {
                value: value.clone(),
                type_: *type_,
            },
            then: Box::new(then),
            else_: Box::new(else_),
        },
        LoweredPattern::Range {
            start,
            end,
            inclusive,
            type_,
        } => LoweredMatchDecision::Test {
            subject: subject.to_string(),
            test: LoweredMatchTest::Range {
                start: start.clone(),
                end: end.clone(),
                inclusive: *inclusive,
                type_: *type_,
            },
            then: Box::new(then),
            else_: Box::new(else_),
        },
        LoweredPattern::Variant {
            enum_name,
            variant,
            payload,
        } => {
            let payload_type = enums
                .get(enum_name)
                .and_then(|enum_| {
                    enum_
                        .variants
                        .iter()
                        .find(|item| item.name == *variant)
                        .and_then(|item| item.payload.clone())
                });
            let after_tag = if let Some(payload) = payload {
                let Some(payload_type) = payload_type else {
                    return else_;
                };
                match payload.as_ref() {
                    LoweredPattern::Binding { name } => LoweredMatchDecision::Bind {
                        name: name.clone(),
                        type_: payload_type,
                        source: LoweredMatchBindSource::EnumPayload {
                            subject: subject.to_string(),
                            variant: variant.clone(),
                        },
                        declare: !assigned_bindings.contains(name),
                        then: Box::new(then),
                    },
                    LoweredPattern::Wildcard => then,
                    _ => {
                        let payload_temp = match_payload_temp_name(subject, temp_counter);
                        LoweredMatchDecision::Bind {
                            name: payload_temp.clone(),
                            type_: payload_type.clone(),
                            source: LoweredMatchBindSource::EnumPayload {
                                subject: subject.to_string(),
                                variant: variant.clone(),
                            },
                            declare: true,
                            then: Box::new(compile_pattern(
                                payload,
                                &payload_temp,
                                &payload_type,
                                then,
                                else_.clone(),
                                enums,
                                structs,
                                temp_counter,
                                assigned_bindings,
                            )),
                        }
                    }
                }
            } else {
                then
            };
            LoweredMatchDecision::Test {
                subject: subject.to_string(),
                test: LoweredMatchTest::EnumTag {
                    enum_name: enum_name.clone(),
                    variant: variant.clone(),
                },
                then: Box::new(after_tag),
                else_: Box::new(else_),
            }
        }
        LoweredPattern::Struct { fields, .. } => {
            let mut decision = then;
            for field in fields.iter().rev() {
                let field_type = match subject_type {
                    LoweredType::Struct(name) => structs
                        .get(name)
                        .and_then(|struct_| {
                            struct_
                                .fields
                                .iter()
                                .find(|item| item.name == field.name)
                                .map(|item| item.type_.clone())
                        })
                        .unwrap_or(LoweredType::Void),
                    _ => LoweredType::Void,
                };
                decision = match &field.pattern {
                    LoweredPattern::Binding { name } => LoweredMatchDecision::Bind {
                        name: name.clone(),
                        type_: field_type,
                        source: LoweredMatchBindSource::StructField {
                            subject: subject.to_string(),
                            field: field.name.clone(),
                        },
                        declare: !assigned_bindings.contains(name),
                        then: Box::new(decision),
                    },
                    LoweredPattern::Wildcard => decision,
                    _ => {
                        let field_temp = match_payload_temp_name(subject, temp_counter);
                        LoweredMatchDecision::Bind {
                            name: field_temp.clone(),
                            type_: field_type.clone(),
                            source: LoweredMatchBindSource::StructField {
                                subject: subject.to_string(),
                                field: field.name.clone(),
                            },
                            declare: true,
                            then: Box::new(compile_pattern(
                                &field.pattern,
                                &field_temp,
                                &field_type,
                                decision,
                                else_.clone(),
                                enums,
                                structs,
                                temp_counter,
                                assigned_bindings,
                            )),
                        }
                    }
                };
            }
            decision
        }
        LoweredPattern::Or(alternatives) => {
            let bindings = pattern_binding_names(pattern)
                .into_iter()
                .filter_map(|name| {
                    let type_ = pattern_binding_type(
                        pattern,
                        &name,
                        subject_type,
                        enums,
                        structs,
                    )?;
                    Some(LoweredMatchOrBinding { name, type_ })
                })
                .collect::<Vec<_>>();
            let assigned = bindings
                .iter()
                .map(|binding| binding.name.clone())
                .collect::<HashSet<_>>();
            let alternatives = alternatives
                .iter()
                .map(|alternative| {
                    compile_pattern(
                        alternative,
                        subject,
                        subject_type,
                        LoweredMatchDecision::Matched,
                        LoweredMatchDecision::Fail,
                        enums,
                        structs,
                        temp_counter,
                        &assigned,
                    )
                })
                .collect();
            LoweredMatchDecision::Or {
                bindings,
                alternatives,
                then: Box::new(then),
                else_: Box::new(else_),
            }
        }
    }
}

fn pattern_binding_names(pattern: &LoweredPattern) -> Vec<String> {
    let mut names = Vec::new();
    collect_pattern_binding_names(pattern, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_pattern_binding_names(pattern: &LoweredPattern, names: &mut Vec<String>) {
    match pattern {
        LoweredPattern::Binding { name } => names.push(name.clone()),
        LoweredPattern::Or(alternatives) => {
            if let Some(first) = alternatives.first() {
                collect_pattern_binding_names(first, names);
            }
        }
        LoweredPattern::Variant { payload, .. } => {
            if let Some(payload) = payload {
                collect_pattern_binding_names(payload, names);
            }
        }
        LoweredPattern::Struct { fields, .. } => {
            for field in fields {
                collect_pattern_binding_names(&field.pattern, names);
            }
        }
        LoweredPattern::String(_)
        | LoweredPattern::Bool(_)
        | LoweredPattern::Number { .. }
        | LoweredPattern::Range { .. }
        | LoweredPattern::Wildcard => {}
    }
}

fn pattern_binding_type(
    pattern: &LoweredPattern,
    name: &str,
    subject_type: &LoweredType,
    enums: &HashMap<String, LoweredEnum>,
    structs: &HashMap<String, LoweredStruct>,
) -> Option<LoweredType> {
    match pattern {
        LoweredPattern::Binding {
            name: binding_name,
        } if binding_name == name => Some(subject_type.clone()),
        LoweredPattern::Or(alternatives) => alternatives.iter().find_map(|alternative| {
            pattern_binding_type(alternative, name, subject_type, enums, structs)
        }),
        LoweredPattern::Variant {
            enum_name,
            variant,
            payload,
        } => {
            let payload = payload.as_ref()?;
            let payload_type = enums
                .get(enum_name)?
                .variants
                .iter()
                .find(|item| item.name == *variant)?
                .payload
                .as_ref()?;
            pattern_binding_type(payload, name, payload_type, enums, structs)
        }
        LoweredPattern::Struct { fields, .. } => {
            let LoweredType::Struct(struct_name) = subject_type else {
                return None;
            };
            let struct_ = structs.get(struct_name)?;
            for field in fields {
                let field_type = struct_
                    .fields
                    .iter()
                    .find(|item| item.name == field.name)?
                    .type_
                    .clone();
                if let Some(type_) =
                    pattern_binding_type(&field.pattern, name, &field_type, enums, structs)
                {
                    return Some(type_);
                }
            }
            None
        }
        _ => None,
    }
}

fn match_payload_temp_name(subject: &str, temp_counter: &mut usize) -> String {
    let name = format!("{subject}_payload_{temp_counter}");
    *temp_counter += 1;
    name
}
