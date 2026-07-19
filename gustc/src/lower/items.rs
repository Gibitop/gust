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
        if trait_method_uses_generic_associated_type(&item.associated_types, method) {
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

fn trait_method_uses_generic_associated_type(
    associated_types: &[crate::ast::AssociatedTypeDecl],
    method: &TraitMethodDecl,
) -> bool {
    method
        .params
        .iter()
        .filter_map(|param| param.type_ref.as_ref())
        .any(|type_ref| type_ref_uses_generic_associated_type(associated_types, type_ref))
        || method.return_type.as_ref().is_some_and(|type_ref| {
            type_ref_uses_generic_associated_type(associated_types, type_ref)
        })
}

fn type_ref_uses_generic_associated_type(
    associated_types: &[crate::ast::AssociatedTypeDecl],
    type_ref: &TypeRef,
) -> bool {
    type_ref
        .name
        .strip_prefix("Self.")
        .is_some_and(|name| {
            !type_ref.args.is_empty()
                && associated_types.iter().any(|associated_type| {
                    associated_type.name == name && !associated_type.type_params.is_empty()
                })
        })
        || type_ref
            .args
            .iter()
            .any(|arg| type_ref_uses_generic_associated_type(associated_types, arg))
        || type_ref.bindings.iter().any(|binding| {
            type_ref_uses_generic_associated_type(associated_types, &binding.type_ref)
        })
        || type_ref.function.as_ref().is_some_and(|function| {
            function.params.iter().any(|param| {
                type_ref_uses_generic_associated_type(associated_types, &param.type_ref)
            }) || type_ref_uses_generic_associated_type(associated_types, &function.return_type)
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
    static_locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredFunction> {
    let signature = signatures.get(name)?;
    let captured_names = captured_let_names(function);
    CAPTURED_NAMES.with(|names| *names.borrow_mut() = captured_names);

    let mut locals = static_locals.clone();
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
                    StmtKind::Block(block) => statements.push(lower_scoped_block_statement(
                        block,
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
    static_locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LoweredStatement> {
    let mut statements = Vec::new();
    let mut locals = static_locals.clone();
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
                    StmtKind::Block(block) => statements.push(lower_scoped_block_statement(
                        block,
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

fn lower_static_vars(
    program: &Program,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (
    Vec<LoweredStaticVar>,
    Vec<LoweredStatement>,
    HashMap<String, LoweringLocal>,
) {
    let mut statics = Vec::new();
    let mut statements = Vec::new();
    let mut static_locals = HashMap::new();

    for item in ordered_static_vars(program) {
        let expected_type = item.type_annotation.as_ref().and_then(|type_ref| {
            lower_value_type_ref(
                type_ref,
                structs,
                enums,
                traits,
                diagnostics,
                "top-level let annotations only support basic, known struct, enum, trait, and function types in executable builds",
            )
        });
        let static_initializer = FunctionDecl {
            name: None,
            exported: false,
            type_params: Vec::new(),
            type_param_bounds: Vec::new(),
            params: Vec::new(),
            return_type: None,
            body: FunctionBody::Expr(Box::new(item.value.clone())),
            span: item.span,
        };
        let captured_names = captured_let_names(&static_initializer);
        CAPTURED_NAMES.with(|names| *names.borrow_mut() = captured_names);
        let Some(value) = lower_expr(
            &item.value,
            &static_locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            expected_type.clone(),
            "expected supported top-level let initializer in executable builds",
        ) else {
            CAPTURED_NAMES.with(|names| names.borrow_mut().clear());
            continue;
        };
        CAPTURED_NAMES.with(|names| names.borrow_mut().clear());
        let type_ = expected_type.unwrap_or_else(|| value.type_.clone());

        statics.push(LoweredStaticVar {
            name: item.name.clone(),
            type_: type_.clone(),
        });
        statements.push(LoweredStatement::Assignment {
            target: LoweredExpr {
                type_: type_.clone(),
                kind: LoweredExprKind::Local(item.name.clone()),
            },
            value,
        });
        static_locals.insert(
            item.name.clone(),
            LoweringLocal {
                type_,
                mutable: false,
                replacement: None,
                captured: false,
            },
        );
    }

    (statics, statements, static_locals)
}

fn ordered_static_vars(program: &Program) -> Vec<&StaticVarDecl> {
    let statics = program
        .items
        .iter()
        .filter_map(|item| {
            let Item::StaticVar(item) = item else {
                return None;
            };
            Some(item)
        })
        .collect::<Vec<_>>();
    let static_names = statics
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();
    let mut functions: HashMap<String, Vec<&FunctionDecl>> = HashMap::new();
    for item in &program.items {
        collect_item_static_dependency_functions(item, &mut functions);
    }
    let dependencies = statics
        .iter()
        .map(|item| {
            let mut deps = HashSet::new();
            collect_expr_static_dependencies(
                &item.value,
                &static_names,
                &functions,
                &mut HashSet::new(),
                &mut deps,
            );
            (item.name.clone(), deps)
        })
        .collect::<HashMap<_, _>>();

    let mut remaining = static_names;
    let mut ordered = Vec::new();

    while !remaining.is_empty() {
        let Some(item) = statics.iter().find(|item| {
            remaining.contains(&item.name)
                && dependencies[&item.name]
                    .iter()
                    .all(|dependency| !remaining.contains(dependency))
        }) else {
            ordered.extend(statics.iter().filter(|item| remaining.remove(&item.name)).copied());
            break;
        };

        remaining.remove(&item.name);
        ordered.push(*item);
    }

    ordered
}

fn collect_item_static_dependency_functions<'program>(
    item: &'program Item,
    functions: &mut HashMap<String, Vec<&'program FunctionDecl>>,
) {
    match item {
        Item::Function(function) => {
            if let Some(name) = &function.name {
                functions.entry(name.clone()).or_default().push(function);
            }
        }
        Item::Struct(item) => {
            for member in &item.members {
                if let StructMember::Method(function) | StructMember::StaticMethod(function) =
                    member
                    && let Some(name) = &function.name
                {
                    functions.entry(name.clone()).or_default().push(function);
                }
            }
        }
        Item::Enum(item) => {
            for member in &item.members {
                if let StructMember::Method(function) | StructMember::StaticMethod(function) =
                    member
                    && let Some(name) = &function.name
                {
                    functions.entry(name.clone()).or_default().push(function);
                }
            }
        }
        Item::Extension(item) => {
            if let Some(name) = &item.function.name {
                functions
                    .entry(name.clone())
                    .or_default()
                    .push(&item.function);
            }
        }
        Item::Impl(item) => {
            for member in &item.methods {
                if let Some(name) = &member.function.name {
                    functions
                        .entry(name.clone())
                        .or_default()
                        .push(&member.function);
                }
            }
        }
        Item::Import(_) | Item::Trait(_) | Item::StaticVar(_) => {}
    }
}

fn collect_expr_static_dependencies(
    expr: &Expr,
    static_names: &HashSet<String>,
    functions: &HashMap<String, Vec<&FunctionDecl>>,
    visiting_functions: &mut HashSet<String>,
    dependencies: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            if static_names.contains(name) {
                dependencies.insert(name.clone());
            }
        }
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_expr_static_dependencies(
                    item,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
            }
        }
        ExprKind::Call { callee, args } => {
            let called_name = match &callee.kind {
                ExprKind::Identifier(name)
                | ExprKind::Member { name, .. }
                | ExprKind::GenericMember { name, .. } => Some(name),
                _ => None,
            };
            if let Some(name) = called_name
                && let Some(called_functions) = functions.get(name)
            {
                for function in called_functions {
                    if visiting_functions.insert(name.clone()) {
                        collect_function_static_dependencies(
                            function,
                            static_names,
                            functions,
                            visiting_functions,
                            dependencies,
                        );
                        visiting_functions.remove(name);
                    }
                }
            }
            collect_expr_static_dependencies(
                callee,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            );
            for arg in args {
                collect_expr_static_dependencies(
                    arg,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
            }
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            collect_expr_static_dependencies(
                object,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            );
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_expr_static_dependencies(
                    &field.value,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
            }
        }
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            collect_expr_static_dependencies(
                start,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            );
            collect_expr_static_dependencies(
                end,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            );
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::PostfixIncrement(operand)
        | ExprKind::Cast { value: operand, .. } => {
            collect_expr_static_dependencies(
                operand,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            );
        }
        ExprKind::Match { value, branches } => {
            collect_expr_static_dependencies(
                value,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            );
            for branch in branches {
                if let Some(guard) = &branch.guard {
                    collect_expr_static_dependencies(
                        guard,
                        static_names,
                        functions,
                        visiting_functions,
                        dependencies,
                    );
                }
                match &branch.body {
                    MatchBranchBody::Expr(expr) => collect_expr_static_dependencies(
                        expr,
                        static_names,
                        functions,
                        visiting_functions,
                        dependencies,
                    ),
                    MatchBranchBody::Block(block) => collect_block_static_dependencies(
                        block,
                        static_names,
                        functions,
                        visiting_functions,
                        dependencies,
                    ),
                }
            }
        }
        ExprKind::Lambda(_) => {}
        ExprKind::Block(block) => collect_block_static_dependencies(
            block,
            static_names,
            functions,
            visiting_functions,
            dependencies,
        ),
        ExprKind::Comptime(expr) => collect_expr_static_dependencies(
            expr,
            static_names,
            functions,
            visiting_functions,
            dependencies,
        ),
        ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn collect_function_static_dependencies(
    function: &FunctionDecl,
    static_names: &HashSet<String>,
    functions: &HashMap<String, Vec<&FunctionDecl>>,
    visiting_functions: &mut HashSet<String>,
    dependencies: &mut HashSet<String>,
) {
    match &function.body {
        FunctionBody::Block(block) => collect_block_static_dependencies(
            block,
            static_names,
            functions,
            visiting_functions,
            dependencies,
        ),
        FunctionBody::Expr(expr) => collect_expr_static_dependencies(
            expr,
            static_names,
            functions,
            visiting_functions,
            dependencies,
        ),
    }
}

fn collect_block_static_dependencies(
    block: &Block,
    static_names: &HashSet<String>,
    functions: &HashMap<String, Vec<&FunctionDecl>>,
    visiting_functions: &mut HashSet<String>,
    dependencies: &mut HashSet<String>,
) {
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { value, .. } | StmtKind::Return { value } => {
                if let Some(value) = value {
                    collect_expr_static_dependencies(
                        value,
                        static_names,
                        functions,
                        visiting_functions,
                        dependencies,
                    );
                }
            }
            StmtKind::Assign { target, value, .. } => {
                collect_expr_static_dependencies(
                    target,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
                collect_expr_static_dependencies(
                    value,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_expr_static_dependencies(
                    condition,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
                collect_block_static_dependencies(
                    then_branch,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => collect_block_static_dependencies(
                            block,
                            static_names,
                            functions,
                            visiting_functions,
                            dependencies,
                        ),
                        ElseBranch::If(statement) => {
                            collect_statement_static_dependencies(
                                statement,
                                static_names,
                                functions,
                                visiting_functions,
                                dependencies,
                            );
                        }
                    }
                }
            }
            StmtKind::While { condition, body } => {
                collect_expr_static_dependencies(
                    condition,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
                collect_block_static_dependencies(
                    body,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
            }
            StmtKind::For { iterable, body, .. } => {
                collect_expr_static_dependencies(
                    iterable,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
                collect_block_static_dependencies(
                    body,
                    static_names,
                    functions,
                    visiting_functions,
                    dependencies,
                );
            }
            StmtKind::Block(block) => collect_block_static_dependencies(
                block,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            ),
            StmtKind::Expr(expr) => collect_expr_static_dependencies(
                expr,
                static_names,
                functions,
                visiting_functions,
                dependencies,
            ),
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

fn collect_statement_static_dependencies(
    statement: &Stmt,
    static_names: &HashSet<String>,
    functions: &HashMap<String, Vec<&FunctionDecl>>,
    visiting_functions: &mut HashSet<String>,
    dependencies: &mut HashSet<String>,
) {
    match &statement.kind {
        StmtKind::If { .. }
        | StmtKind::While { .. }
        | StmtKind::For { .. }
        | StmtKind::Let { .. }
        | StmtKind::Assign { .. }
        | StmtKind::Return { .. }
        | StmtKind::Block(_)
        | StmtKind::Expr(_) => collect_block_static_dependencies(
            &Block {
                statements: vec![statement.clone()],
                span: statement.span,
            },
            static_names,
            functions,
            visiting_functions,
            dependencies,
        ),
        StmtKind::Break | StmtKind::Continue => {}
    }
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
