// Loop lowering threads the same locals, signatures, type tables, diagnostics, and return context
// as statement lowering, so keeping these arguments explicit matches the surrounding lowering API.
#[allow(clippy::too_many_arguments)]
fn lower_while_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    let StmtKind::While { condition, body } = &statement.kind else {
        return None;
    };

    let condition = lower_expr(
        condition,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        Some(LoweredType::Basic(BasicType::Bool)),
        "expected supported `while` condition in executable builds",
    )?;
    let body = lower_conditional_block(
        body,
        &mut locals.clone(),
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        return_type,
    );

    Some(LoweredStatement::While { condition, body })
}

fn iteration_trait_name_matches(trait_name: &str, protocol: &str) -> bool {
    let Some((trait_head, _)) = trait_name.split_once('<') else {
        return false;
    };

    source_callable_name(trait_head) == protocol
}

fn iterator_trait_for<'a>(
    type_: &LoweredType,
    traits: &'a HashMap<String, LoweredTrait>,
) -> Option<&'a LoweredTrait> {
    if let LoweredType::Trait(trait_name) = type_
        && iteration_trait_name_matches(trait_name, "Iterator")
    {
        return traits.get(trait_name);
    }

    traits.values().find(|trait_| {
        iteration_trait_name_matches(&trait_.name, "Iterator")
            && trait_.impls.iter().any(|impl_| impl_.self_type == *type_)
    })
}

fn iterable_trait_for<'a>(
    type_: &LoweredType,
    traits: &'a HashMap<String, LoweredTrait>,
) -> Option<&'a LoweredTrait> {
    if let LoweredType::Trait(trait_name) = type_
        && iteration_trait_name_matches(trait_name, "Iterable")
    {
        return traits.get(trait_name);
    }

    traits.values().find(|trait_| {
        iteration_trait_name_matches(&trait_.name, "Iterable")
            && trait_.impls.iter().any(|impl_| impl_.self_type == *type_)
    })
}

fn coerce_to_iterator(
    value: LoweredExpr,
    iterator_trait: &LoweredTrait,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let iterator_type = LoweredType::Trait(iterator_trait.name.clone());
    if value.type_ == iterator_type {
        return Some(value);
    }

    if !matches!(value.type_, LoweredType::Struct(_) | LoweredType::Enum(_))
        || !iterator_trait
            .impls
            .iter()
            .any(|impl_| impl_.self_type == value.type_)
    {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "iteration method must return `{}`, got `{}`",
                iterator_type.name(),
                value.type_.name()
            ),
        ));
        return None;
    }

    Some(LoweredExpr {
        type_: iterator_type,
        kind: LoweredExprKind::TraitObject {
            trait_name: iterator_trait.name.clone(),
            self_type: value.type_.clone(),
            value: Box::new(value),
        },
    })
}

// `for` lowering needs the full executable lowering environment to resolve iterator traits,
// synthesize calls, and lower the loop body without hiding control-flow context in globals.
#[allow(clippy::too_many_arguments)]
fn lower_for_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Vec<LoweredStatement> {
    let StmtKind::For {
        name,
        iterable,
        body,
    } = &statement.kind
    else {
        return Vec::new();
    };

    let Some(iterable) = lower_expr(
        iterable,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        None,
        "expected an iterable value in executable builds",
    ) else {
        return Vec::new();
    };

    let iterator = if let Some(iterator_trait) = iterator_trait_for(&iterable.type_, traits) {
        coerce_to_iterator(iterable, iterator_trait, statement.span, diagnostics)
    } else {
        let Some(iterable_trait) = iterable_trait_for(&iterable.type_, traits) else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "`for` requires an `Iterator<T>` or `Iterable<T>`, got `{}`",
                    iterable.type_.name()
                ),
            ));
            return Vec::new();
        };
        let Some(iterator_method) = iterable_trait
            .methods
            .iter()
            .find(|method| method.name == "iterator")
        else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "trait `{}` does not define `iterator()`",
                    iterable_trait.name
                ),
            ));
            return Vec::new();
        };
        let LoweredType::Trait(iterator_trait_name) = &iterator_method.return_type else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "`{}.iterator()` must return an `Iterator<T>`",
                    iterable_trait.name
                ),
            ));
            return Vec::new();
        };
        let Some(iterator_trait) = traits.get(iterator_trait_name) else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!("unknown iterator trait `{iterator_trait_name}`"),
            ));
            return Vec::new();
        };
        if !iteration_trait_name_matches(&iterator_trait.name, "Iterator") {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "`{}.iterator()` must return an `Iterator<T>`",
                    iterable_trait.name
                ),
            ));
            return Vec::new();
        }

        let iterator_value = if matches!(iterable.type_, LoweredType::Trait(_)) {
            LoweredExpr {
                type_: iterator_method.return_type.clone(),
                kind: LoweredExprKind::DynamicCall {
                    object: Box::new(iterable),
                    method: "iterator".to_string(),
                    args: Vec::new(),
                },
            }
        } else {
            let Some(function_name) =
                trait_impl_method_name(iterable_trait, &iterable.type_, "iterator")
            else {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    format!(
                        "`{}` does not implement `{}::iterator()`",
                        iterable.type_.name(),
                        iterable_trait.name
                    ),
                ));
                return Vec::new();
            };
            let Some(signature) = signatures.get(&function_name) else {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "failed to lower the iterable `iterator()` implementation",
                ));
                return Vec::new();
            };
            LoweredExpr {
                type_: signature.return_type.clone(),
                kind: LoweredExprKind::Call {
                    name: function_name,
                    args: vec![iterable],
                },
            }
        };

        coerce_to_iterator(iterator_value, iterator_trait, statement.span, diagnostics)
    };
    let Some(iterator) = iterator else {
        return Vec::new();
    };

    let LoweredType::Trait(iterator_trait_name) = &iterator.type_ else {
        unreachable!("iterator coercion must produce a trait object");
    };
    let Some(iterator_trait) = traits.get(iterator_trait_name) else {
        return Vec::new();
    };
    let Some(next_method) = iterator_trait
        .methods
        .iter()
        .find(|method| method.name == "next")
    else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("trait `{}` does not define `next()`", iterator_trait.name),
        ));
        return Vec::new();
    };
    let LoweredType::Enum(option_name) = &next_method.return_type else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("`{}.next()` must return `Option<T>`", iterator_trait.name),
        ));
        return Vec::new();
    };
    let Some(option) = enums.get(option_name) else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("unknown iterator result enum `{option_name}`"),
        ));
        return Vec::new();
    };
    let Some(item_type) = option
        .variants
        .iter()
        .find(|variant| variant.name == "Some")
        .and_then(|variant| variant.payload.clone())
    else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("iterator result enum `{option_name}` must define `Some(T)`"),
        ));
        return Vec::new();
    };
    if !option.variants.iter().any(|variant| variant.name == "None") {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("iterator result enum `{option_name}` must define `None`"),
        ));
        return Vec::new();
    }

    let iterator_name = for_iterator_local_name(statement.span);
    let match_value_name = match_temp_name(statement.span);
    let mut loop_locals = locals.clone();
    loop_locals.insert(
        iterator_name.clone(),
        LoweringLocal {
            type_: iterator.type_.clone(),
            mutable: true,
            replacement: None,
            captured: false,
        },
    );
    let mut item_locals = loop_locals.clone();
    item_locals.insert(
        name.clone(),
        LoweringLocal {
            type_: item_type.clone(),
            mutable: false,
            replacement: Some(LoweredExpr {
                type_: item_type.clone(),
                kind: LoweredExprKind::EnumPayload {
                    object: Box::new(LoweredExpr {
                        type_: next_method.return_type.clone(),
                        kind: LoweredExprKind::MatchValue(match_value_name.clone()),
                    }),
                    variant: "Some".to_string(),
                },
            }),
            captured: false,
        },
    );
    let body = lower_conditional_block(
        body,
        &mut item_locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        return_type,
    );
    let next = LoweredExpr {
        type_: next_method.return_type.clone(),
        kind: LoweredExprKind::DynamicCall {
            object: Box::new(LoweredExpr {
                type_: iterator.type_.clone(),
                kind: LoweredExprKind::Local(iterator_name.clone()),
            }),
            method: "next".to_string(),
            args: Vec::new(),
        },
    };

    vec![
        LoweredStatement::Local {
            name: iterator_name,
            value: iterator,
        },
        LoweredStatement::While {
            condition: LoweredExpr {
                type_: LoweredType::Basic(BasicType::Bool),
                kind: LoweredExprKind::BoolLiteral(true),
            },
            body: vec![LoweredStatement::Match {
                value: next,
                temp_name: match_value_name,
                branches: vec![
                    LoweredMatchStatementBranch {
                        pattern: LoweredPattern::Variant {
                            enum_name: option_name.clone(),
                            variant: "Some".to_string(),
                            payload: None,
                        },
                        guard: None,
                        statements: body,
                    },
                    LoweredMatchStatementBranch {
                        pattern: LoweredPattern::Variant {
                            enum_name: option_name.clone(),
                            variant: "None".to_string(),
                            payload: None,
                        },
                        guard: None,
                        statements: vec![LoweredStatement::Break],
                    },
                ],
            }],
        },
    ]
}
