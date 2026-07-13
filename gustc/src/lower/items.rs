fn is_self_param(param: &crate::ast::Param) -> bool {
    param.name == "self"
}

fn has_mutable_receiver(function: &FunctionDecl) -> bool {
    function
        .params
        .iter()
        .any(|param| is_self_param(param) && param.mutable && param.type_ref.is_none())
}

fn lower_trait_definition(
    item: &TraitDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredTrait> {
    let mut methods = Vec::new();
    let mut can_lower = true;

    for method in &item.methods {
        if method.static_ {
            continue;
        }

        let Some(method) =
            lower_trait_method_definition(method, structs, enums, traits, diagnostics)
        else {
            can_lower = false;
            continue;
        };
        methods.push(method);
    }

    can_lower.then(|| LoweredTrait {
        name: item.name.clone(),
        methods,
        impls: Vec::new(),
    })
}

fn lower_trait_method_definition(
    method: &TraitMethodDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredTraitMethod> {
    let mut params = Vec::new();
    let mut can_lower = true;

    for param in method.params.iter().filter(|param| !is_self_param(param)) {
        let Some(type_ref) = &param.type_ref else {
            diagnostics.push(Diagnostic::error(
                param.span,
                "trait method parameters must include type annotations in executable builds",
            ));
            can_lower = false;
            continue;
        };

        let Some(type_) = lower_trait_object_type_ref(
            type_ref,
            structs,
            enums,
            traits,
            diagnostics,
            "trait object methods only support basic, known struct, enum, trait, and function parameter types in executable builds",
        ) else {
            can_lower = false;
            continue;
        };

        params.push(LoweredParamSignature {
            type_,
            mutable: param.mutable,
        });
    }

    let return_type = method.return_type.as_ref().and_then(|return_type| {
        lower_trait_object_type_ref(
            return_type,
            structs,
            enums,
            traits,
            diagnostics,
            "trait object methods only support basic, known struct, enum, trait, and function return types in executable builds",
        )
    })?;

    can_lower.then(|| LoweredTraitMethod {
        name: method.name.clone(),
        params,
        return_type,
        mutable_self: has_trait_mutable_receiver(method),
    })
}

fn lower_trait_object_type_ref(
    type_ref: &TypeRef,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if type_ref.name == "Self" && type_ref.args.is_empty() {
        diagnostics.push(Diagnostic::error(
            type_ref.span,
            "`Self` is not supported in dynamically dispatched trait method signatures yet",
        ));
        return None;
    }

    lower_value_type_ref(type_ref, structs, enums, traits, diagnostics, message)
}

fn has_trait_mutable_receiver(method: &TraitMethodDecl) -> bool {
    method
        .params
        .iter()
        .any(|param| is_self_param(param) && param.mutable && param.type_ref.is_none())
}

fn lower_enum_definition(
    item: &crate::ast::EnumDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredEnum> {
    let mut variants = Vec::new();
    let mut variant_names = HashMap::new();
    let mut can_lower = true;

    if item.variants.is_empty() {
        diagnostics.push(Diagnostic::error(
            item.span,
            format!("enum `{}` must define at least one variant", item.name),
        ));
        can_lower = false;
    }

    for variant in &item.variants {
        if variant_names
            .insert(variant.name.clone(), variant.span)
            .is_some()
        {
            diagnostics.push(Diagnostic::error(
                variant.span,
                format!(
                    "duplicate variant `{}` in enum `{}`",
                    variant.name, item.name
                ),
            ));
            can_lower = false;
        }

        let payload = variant.payload.as_ref().and_then(|type_ref| {
            lower_value_type_ref(
                type_ref,
                structs,
                enums,
                traits,
                diagnostics,
                "enum payloads only support basic and known struct or enum types in executable builds",
            )
        });

        if variant.payload.is_some() && payload.is_none() {
            can_lower = false;
        }

        variants.push(LoweredVariant {
            name: variant.name.clone(),
            payload,
        });
    }

    can_lower.then(|| LoweredEnum {
        name: item.name.clone(),
        variants,
    })
}

fn lower_struct_definition(
    item: &StructDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredStruct> {
    let mut fields = Vec::new();
    let mut field_names = HashMap::new();
    let mut can_lower = true;

    for member in &item.members {
        match member {
            StructMember::Field(field) => {
                if field_names.insert(field.name.clone(), field.span).is_some() {
                    diagnostics.push(Diagnostic::error(
                        field.span,
                        format!("duplicate field `{}` in struct `{}`", field.name, item.name),
                    ));
                    can_lower = false;
                }

                let Some(type_) = lower_value_type_ref(
                    &field.type_ref,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    "struct fields only support basic, known struct, enum, trait, and function types in executable builds",
                ) else {
                    can_lower = false;
                    continue;
                };

                fields.push(LoweredField {
                    name: field.name.clone(),
                    type_,
                });
            }
            StructMember::Method(_) | StructMember::StaticMethod(_) => {}
        }
    }

    if can_lower {
        Some(LoweredStruct {
            name: item.name.clone(),
            fields,
            raw_buffer_element: None,
        })
    } else {
        None
    }
}

fn lower_function_signature(
    function: &FunctionDecl,
    self_type: Option<&LoweredType>,
    has_self: bool,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSignature> {
    let mut params = Vec::new();
    let mut can_lower = true;

    for param in function
        .params
        .iter()
        .filter(|param| !has_self || !is_self_param(param))
    {
        let Some(type_ref) = &param.type_ref else {
            diagnostics.push(Diagnostic::error(
                param.span,
                "function parameters must include type annotations in executable builds",
            ));
            can_lower = false;
            continue;
        };

        let Some(type_) = lower_value_type_ref_in_context(
            type_ref,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "only basic, known struct, enum, trait, and function parameter types are supported in executable builds",
        ) else {
            can_lower = false;
            continue;
        };

        params.push(LoweredParamSignature {
            type_,
            mutable: param.mutable,
        });
    }

    let (return_type, return_type_known) = if let Some(return_type) = &function.return_type {
        let return_type = lower_value_type_ref_in_context(
            return_type,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "only basic, known struct, enum, trait, and function return types are supported in executable builds",
        )?;
        (return_type, true)
    } else {
        (LoweredType::Void, false)
    };

    if can_lower {
        Some(FunctionSignature {
            params,
            return_type,
            return_type_known,
            mutable_self: has_self && has_mutable_receiver(function),
        })
    } else {
        None
    }
}

// Function lowering is the boundary where signatures, concrete type tables, diagnostics, and
// optional receiver context come together to produce executable IR.
#[allow(clippy::too_many_arguments)]
fn lower_function(
    function: &FunctionDecl,
    name: &str,
    self_type: Option<&LoweredType>,
    has_self: bool,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredFunction> {
    let signature = signatures.get(name)?;
    let captured_names = captured_let_names(function);
    CAPTURED_NAMES.with(|names| *names.borrow_mut() = captured_names);

    let mut locals = HashMap::new();
    let mut params = Vec::new();
    let mut statements = Vec::new();
    let mut return_value = None;

    if let Some(self_type) = self_type {
        locals.insert(
            "Self".to_string(),
            LoweringLocal {
                type_: self_type.clone(),
                mutable: false,
                replacement: None,
                captured: false,
            },
        );
    }

    let signature_params = if has_self {
        let self_type = self_type?;
        let self_param = signature.params.first()?;
        locals.insert(
            "self".to_string(),
            LoweringLocal {
                type_: self_type.clone(),
                mutable: signature.mutable_self,
                replacement: None,
                captured: false,
            },
        );
        params.push(LoweredParam {
            name: "self".to_string(),
            type_: self_param.type_.clone(),
        });
        &signature.params[1..]
    } else {
        &signature.params[..]
    };

    for (param, signature_param) in function
        .params
        .iter()
        .filter(|param| !has_self || !is_self_param(param))
        .zip(signature_params)
    {
        if locals
            .insert(
                param.name.clone(),
                LoweringLocal {
                    type_: signature_param.type_.clone(),
                    mutable: param.mutable,
                    replacement: None,
                    captured: false,
                },
            )
            .is_some()
        {
            diagnostics.push(Diagnostic::error(
                param.span,
                format!("duplicate local `{}` in executable build", param.name),
            ));
        }

        params.push(LoweredParam {
            name: param.name.clone(),
            type_: signature_param.type_.clone(),
        });
    }

    match &function.body {
        FunctionBody::Expr(expr) => {
            return_value = lower_expr(
                expr,
                &locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                Some(signature.return_type.clone()),
                "expected supported arrow function value in executable builds",
            );
        }
        FunctionBody::Block(block) => {
            for (index, statement) in block.statements.iter().enumerate() {
                let is_last = index + 1 == block.statements.len();

                match &statement.kind {
                    StmtKind::Let { .. } => {
                        if let Some(statement) = lower_local_statement(
                            statement,
                            &mut locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Assign { .. } => {
                        if let Some(statement) = lower_assignment_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Return { value } => {
                        let value = value.as_ref().and_then(|value| {
                            lower_expr(
                                value,
                                &locals,
                                signatures,
                                structs,
                                enums,
                                traits,
                                diagnostics,
                                Some(signature.return_type.clone()),
                                "expected supported return value in executable builds",
                            )
                        });

                        if is_last && value.is_some() {
                            return_value = value;
                        } else {
                            statements.push(LoweredStatement::Return(value));
                        }
                    }
                    StmtKind::Expr(expr) => {
                        if let Some(statement) = lower_expression_statement(
                            expr,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            Some(&signature.return_type),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::If { .. } => {
                        if let Some(statement) = lower_if_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            Some(&signature.return_type),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::While { .. } => {
                        if let Some(statement) = lower_while_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            Some(&signature.return_type),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Break => statements.push(LoweredStatement::Break),
                    StmtKind::Continue => statements.push(LoweredStatement::Continue),
                    StmtKind::For { .. } => statements.extend(lower_for_statement(
                        statement,
                        &locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(&signature.return_type),
                    )),
                }
            }
        }
    }

    let return_value = return_value.unwrap_or_else(void_expr);

    if signature.return_type != LoweredType::Void
        && return_value.type_ == LoweredType::Void
        && !statements
            .iter()
            .any(lowered_statement_always_returns_value)
    {
        diagnostics.push(Diagnostic::error(
            function.span,
            "function must return a value in executable builds",
        ));
    }

    Some(LoweredFunction {
        name: name.to_string(),
        location: lower_source_location(function.span),
        params,
        return_type: signature.return_type.clone(),
        statements,
        return_value,
    })
}

fn lower_main(
    main: &FunctionDecl,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LoweredStatement> {
    let mut statements = Vec::new();
    let mut locals = HashMap::new();
    let captured_names = captured_let_names(main);
    CAPTURED_NAMES.with(|names| *names.borrow_mut() = captured_names);

    if let Some(param) = main.params.first() {
        diagnostics.push(Diagnostic::error(
            param.span,
            "`main` parameters are not supported in executable builds",
        ));
    }

    if let Some(return_type) = &main.return_type {
        diagnostics.push(Diagnostic::error(
            return_type.span,
            "`main` return types are not supported in executable builds",
        ));
    }

    match &main.body {
        FunctionBody::Block(block) => {
            for statement in &block.statements {
                match &statement.kind {
                    StmtKind::Expr(expr) => {
                        if let Some(statement) = lower_expression_statement(
                            expr,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            None,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Let { .. } => {
                        if let Some(statement) = lower_local_statement(
                            statement,
                            &mut locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Assign { .. } => {
                        if let Some(statement) = lower_assignment_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Return { .. } => {
                        diagnostics.push(Diagnostic::error(
                            statement.span,
                            "return statements are not supported in executable builds",
                        ));
                    }
                    StmtKind::If { .. } => {
                        if let Some(statement) = lower_if_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            None,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::While { .. } => {
                        if let Some(statement) = lower_while_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            None,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Break => statements.push(LoweredStatement::Break),
                    StmtKind::Continue => statements.push(LoweredStatement::Continue),
                    StmtKind::For { .. } => statements.extend(lower_for_statement(
                        statement,
                        &locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        None,
                    )),
                }
            }
        }
        FunctionBody::Expr(expr) => diagnostics.push(Diagnostic::error(
            expr.span,
            "arrow function bodies are not supported in executable builds",
        )),
    }

    statements
}

fn lower_function_value_expr(
    name: &str,
    signatures: &HashMap<String, FunctionSignature>,
    expected_type: Option<LoweredType>,
    diagnostics: &mut Vec<Diagnostic>,
    span: Span,
) -> Option<LoweredExpr> {
    let signature = signatures.get(name)?;
    let function_type = LoweredType::Function {
        params: signature
            .params
            .iter()
            .map(|param| LoweredFunctionTypeParam {
                type_: param.type_.clone(),
                mutable: param.mutable,
            })
            .collect(),
        return_type: Box::new(signature.return_type.clone()),
    };

    if let Some(expected_type) = expected_type
        && expected_type != function_type
    {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "expected value of type `{}`, got `{}`",
                expected_type.name(),
                function_type.name()
            ),
        ));
        return None;
    }

    let wrapper_name = CLOSURE_LOWERING.with(|state| {
        let mut state = state.borrow_mut();
        let wrapper_name = format!("functionValue{}_{}", state.next_closure_id, name);
        state.next_closure_id += 1;

        let params = signature
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| LoweredParam {
                name: format!("arg{index}"),
                type_: param.type_.clone(),
            })
            .collect::<Vec<_>>();
        let args = params
            .iter()
            .map(|param| LoweredExpr {
                type_: param.type_.clone(),
                kind: LoweredExprKind::Local(param.name.clone()),
            })
            .collect::<Vec<_>>();

        state.closure_functions.push(LoweredClosureFunction {
            name: wrapper_name.clone(),
            captures: Vec::new(),
            params,
            return_type: signature.return_type.clone(),
            statements: Vec::new(),
            return_value: LoweredExpr {
                type_: signature.return_type.clone(),
                kind: LoweredExprKind::Call {
                    name: name.to_string(),
                    args,
                    location: lower_source_location(span),
                },
            },
        });

        wrapper_name
    });

    Some(LoweredExpr {
        type_: function_type,
        kind: LoweredExprKind::Closure {
            name: wrapper_name,
            captures: Vec::new(),
        },
    })
}
