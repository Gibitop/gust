fn lower_value_type_ref_in_context(
    type_ref: &TypeRef,
    self_type: Option<&LoweredType>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if let Some(function) = &type_ref.function {
        let mut params = Vec::new();
        let mut can_lower = true;
        for param in &function.params {
            let Some(type_) = lower_value_type_ref_in_context(
                &param.type_ref,
                self_type,
                structs,
                enums,
                traits,
                diagnostics,
                message,
            ) else {
                can_lower = false;
                continue;
            };
            params.push(LoweredFunctionTypeParam {
                type_,
                mutable: param.mutable,
            });
        }
        let return_type = lower_value_type_ref_in_context(
            &function.return_type,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            message,
        );
        return if can_lower {
            return_type.map(|return_type| LoweredType::Function {
                params,
                return_type: Box::new(return_type),
            })
        } else {
            None
        };
    }

    if type_ref.name == "Self" && type_ref.args.is_empty() {
        return self_type.cloned().or_else(|| {
            diagnostics.push(Diagnostic::error(
                type_ref.span,
                "`Self` is only available in methods and extension functions",
            ));
            None
        });
    }

    lower_value_type_ref(type_ref, structs, enums, traits, diagnostics, message)
}

fn lower_value_type_ref(
    type_ref: &TypeRef,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if let Some(function) = &type_ref.function {
        let mut params = Vec::new();
        let mut can_lower = true;
        for param in &function.params {
            let Some(type_) = lower_value_type_ref(
                &param.type_ref,
                structs,
                enums,
                traits,
                diagnostics,
                message,
            ) else {
                can_lower = false;
                continue;
            };
            params.push(LoweredFunctionTypeParam {
                type_,
                mutable: param.mutable,
            });
        }
        let return_type = lower_value_type_ref(
            &function.return_type,
            structs,
            enums,
            traits,
            diagnostics,
            message,
        );
        return if can_lower {
            return_type.map(|return_type| LoweredType::Function {
                params,
                return_type: Box::new(return_type),
            })
        } else {
            None
        };
    }

    if !type_ref.args.is_empty() {
        diagnostics.push(Diagnostic::error(
            type_ref.span,
            "generic types are not supported in executable builds",
        ));
        return None;
    }

    if let Some(type_) = BasicType::from_name(&type_ref.name) {
        return Some(LoweredType::Basic(type_));
    }

    if type_ref.name == "void" {
        return Some(LoweredType::Void);
    }

    if structs.contains_key(&type_ref.name) {
        return Some(LoweredType::Struct(type_ref.name.clone()));
    }

    if enums.contains_key(&type_ref.name) {
        return Some(LoweredType::Enum(type_ref.name.clone()));
    }

    if traits.contains_key(&type_ref.name) {
        return Some(LoweredType::Trait(type_ref.name.clone()));
    }

    diagnostics.push(Diagnostic::error(type_ref.span, message));
    None
}

fn infer_expr_type(
    expr: &Expr,
    locals: &HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    match &expr.kind {
        ExprKind::String(_) => Some(LoweredType::Basic(BasicType::String)),
        ExprKind::Char(_) => Some(LoweredType::Basic(BasicType::Char)),
        ExprKind::Bool(_)
        | ExprKind::Unary {
            op: UnaryOp::Not, ..
        }
        | ExprKind::Binary {
            op:
                BinaryOp::LogicalAnd
                | BinaryOp::LogicalOr
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual,
            ..
        } => Some(LoweredType::Basic(BasicType::Bool)),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            operand,
        } => infer_expr_type(operand, locals, signatures, structs, enums, traits),
        ExprKind::Cast { type_ref, .. } => lower_value_type_ref(
            type_ref,
            structs,
            enums,
            traits,
            &mut Vec::new(),
            "unsupported cast target type in executable builds",
        ),
        ExprKind::Binary {
            left,
            op:
                BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight,
            right,
        } => {
            if number_pair_contains_float(left, right) {
                Some(LoweredType::Basic(BasicType::F64))
            } else if matches!(left.kind, ExprKind::Number(_))
                && !matches!(right.kind, ExprKind::Number(_))
            {
                infer_expr_type(right, locals, signatures, structs, enums, traits)
            } else {
                infer_expr_type(left, locals, signatures, structs, enums, traits)
            }
        }
        ExprKind::Number(value) => Some(LoweredType::Basic(if number_literal_is_float(value) {
            BasicType::F64
        } else {
            BasicType::I32
        })),
        ExprKind::Identifier(name) => locals.get(name).cloned().or_else(|| {
            let signature = signatures.get(name)?;
            signature
                .return_type_known
                .then(|| function_type_from_signature(signature))
        }),
        ExprKind::StructInit { name, .. } => {
            let name = if name == "Self" {
                let LoweredType::Struct(name) = locals.get("Self")? else {
                    return None;
                };
                name
            } else {
                name
            };
            structs
                .contains_key(name)
                .then(|| LoweredType::Struct(name.clone()))
        }
        ExprKind::Range { inclusive, .. } => {
            let source_name = if *inclusive {
                "RangeInclusive"
            } else {
                "Range"
            };
            find_lowered_struct_by_source_name(source_name, structs).map(LoweredType::Struct)
        }
        ExprKind::Member { object, name } => {
            if let ExprKind::Identifier(enum_name) = &object.kind
                && let Some(variant) = find_qualified_variant(enums, enum_name, name)
                && variant.payload.is_none()
            {
                return Some(LoweredType::Enum(enum_name.clone()));
            }

            let LoweredType::Struct(struct_name) =
                infer_expr_type(object, locals, signatures, structs, enums, traits)?
            else {
                return None;
            };
            structs
                .get(&struct_name)?
                .fields
                .iter()
                .find(|field| field.name == *name)
                .map(|field| field.type_.clone())
        }
        ExprKind::GenericMember { object, .. } => {
            infer_expr_type(object, locals, signatures, structs, enums, traits)
        }
        ExprKind::Call { callee, .. } => {
            if let ExprKind::Member { object, name } = &callee.kind
                && name == "clone"
            {
                return infer_expr_type(object, locals, signatures, structs, enums, traits);
            }

            if let ExprKind::Member { object, name } = &callee.kind
                && let ExprKind::Identifier(enum_name) = &object.kind
                && find_qualified_variant(enums, enum_name, name).is_some()
            {
                return Some(LoweredType::Enum(enum_name.clone()));
            }

            if let ExprKind::Member { object, name } = &callee.kind
                && let Some(type_) = infer_type_expression(object, locals, structs, enums)
            {
                return signatures
                    .get(&callable_static_name(&type_, name, signatures)?)
                    .filter(|signature| signature.return_type_known)
                    .map(|signature| signature.return_type.clone());
            }

            if let ExprKind::Member { object, name } = &callee.kind {
                let object_type =
                    infer_expr_type(object, locals, signatures, structs, enums, traits)?;
                if let LoweredType::Trait(trait_name) = &object_type {
                    return traits
                        .get(trait_name)?
                        .methods
                        .iter()
                        .find(|method| method.name == *name)
                        .map(|method| method.return_type.clone());
                }
                return signatures
                    .get(&callable_method_name(&object_type, name, signatures)?)
                    .filter(|signature| signature.return_type_known)
                    .map(|signature| signature.return_type.clone());
            }

            if let Some(LoweredType::Function { return_type, .. }) =
                infer_expr_type(callee, locals, signatures, structs, enums, traits)
            {
                return Some(*return_type);
            }

            let ExprKind::Identifier(name) = &callee.kind else {
                return None;
            };

            signatures
                .get(name)
                .filter(|signature| signature.return_type_known)
                .map(|signature| signature.return_type.clone())
        }
        ExprKind::Array(_)
        | ExprKind::CollectionLiteral { .. }
        | ExprKind::GenericType { .. }
        | ExprKind::Missing => None,
        ExprKind::Block(block) => {
            let mut block_locals = locals.clone();
            let (last_statement, setup_statements) = block.statements.split_last()?;
            for statement in setup_statements {
                if let StmtKind::Let {
                    name,
                    type_annotation,
                    value,
                    ..
                } = &statement.kind
                {
                    let type_ = type_annotation
                        .as_ref()
                        .and_then(|type_ref| {
                            quiet_lower_type_ref(type_ref, &block_locals, structs, enums, traits)
                        })
                        .or_else(|| {
                            value.as_ref().and_then(|value| {
                                infer_expr_type(
                                    value,
                                    &block_locals,
                                    signatures,
                                    structs,
                                    enums,
                                    traits,
                                )
                            })
                        });
                    if let Some(type_) = type_ {
                        block_locals.insert(name.clone(), type_);
                    }
                }
            }
            let StmtKind::Return { value: Some(value) } = &last_statement.kind else {
                return None;
            };
            infer_expr_type(value, &block_locals, signatures, structs, enums, traits)
        }
        ExprKind::Comptime(expr) => infer_expr_type(expr, locals, signatures, structs, enums, traits),
        ExprKind::Lambda(function) => {
            infer_lambda_expr_type(function, locals, signatures, structs, enums, traits)
        }
        ExprKind::Match { branches, .. } => branches.iter().find_map(|branch| {
            let MatchBranchBody::Expr(expr) = &branch.body else {
                return None;
            };
            infer_expr_type(expr, locals, signatures, structs, enums, traits)
        }),
        ExprKind::PostfixIncrement(target) => {
            infer_expr_type(target, locals, signatures, structs, enums, traits)
        }
    }
}

fn function_type_from_signature(signature: &FunctionSignature) -> LoweredType {
    LoweredType::Function {
        params: signature
            .params
            .iter()
            .map(|param| LoweredFunctionTypeParam {
                type_: param.type_.clone(),
                mutable: param.mutable,
            })
            .collect(),
        return_type: Box::new(signature.return_type.clone()),
    }
}

fn infer_lambda_expr_type(
    function: &FunctionDecl,
    outer_locals: &HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    let mut locals = outer_locals.clone();
    let mut params = Vec::new();
    for param in &function.params {
        let type_ = quiet_lower_type_ref(
            param.type_ref.as_ref()?,
            outer_locals,
            structs,
            enums,
            traits,
        )?;
        locals.insert(param.name.clone(), type_.clone());
        params.push(LoweredFunctionTypeParam {
            type_,
            mutable: param.mutable,
        });
    }

    let return_type = if let Some(type_ref) = &function.return_type {
        quiet_lower_type_ref(type_ref, outer_locals, structs, enums, traits)?
    } else {
        match &function.body {
            FunctionBody::Expr(expr) => {
                infer_expr_type(expr, &locals, signatures, structs, enums, traits)?
            }
            FunctionBody::Block(block) => {
                let mut return_type = None;
                let mut has_unresolved_value_return = false;
                infer_block_return_types(
                    block,
                    &mut locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    &mut return_type,
                    &mut has_unresolved_value_return,
                )
                .ok()?;
                if has_unresolved_value_return {
                    return None;
                }
                return_type.unwrap_or(LoweredType::Void)
            }
        }
    };

    Some(LoweredType::Function {
        params,
        return_type: Box::new(return_type),
    })
}

fn quiet_lower_type_ref(
    type_ref: &TypeRef,
    locals: &HashMap<String, LoweredType>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    if let Some(function) = &type_ref.function {
        let params = function
            .params
            .iter()
            .map(|param| {
                Some(LoweredFunctionTypeParam {
                    type_: quiet_lower_type_ref(&param.type_ref, locals, structs, enums, traits)?,
                    mutable: param.mutable,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let return_type =
            quiet_lower_type_ref(&function.return_type, locals, structs, enums, traits)?;
        return Some(LoweredType::Function {
            params,
            return_type: Box::new(return_type),
        });
    }

    if !type_ref.args.is_empty() {
        return None;
    }
    if type_ref.name == "Self" {
        return locals.get("Self").cloned();
    }
    if let Some(type_) = BasicType::from_name(&type_ref.name) {
        Some(LoweredType::Basic(type_))
    } else if type_ref.name == "void" {
        Some(LoweredType::Void)
    } else if structs.contains_key(&type_ref.name) {
        Some(LoweredType::Struct(type_ref.name.clone()))
    } else if enums.contains_key(&type_ref.name) {
        Some(LoweredType::Enum(type_ref.name.clone()))
    } else if traits.contains_key(&type_ref.name) {
        Some(LoweredType::Trait(type_ref.name.clone()))
    } else {
        None
    }
}

fn infer_type_expression(
    expr: &Expr,
    locals: &HashMap<String, LoweredType>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
) -> Option<LoweredType> {
    let ExprKind::Identifier(name) = &expr.kind else {
        return None;
    };

    if name == "Self" {
        return locals.get(name).cloned();
    }
    if locals.contains_key(name) {
        return None;
    }
    if let Some(type_) = BasicType::from_name(name) {
        Some(LoweredType::Basic(type_))
    } else if structs.contains_key(name) {
        Some(LoweredType::Struct(name.clone()))
    } else if enums.contains_key(name) {
        Some(LoweredType::Enum(name.clone()))
    } else {
        None
    }
}

fn lower_type_expression(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
) -> Option<LoweredType> {
    let ExprKind::Identifier(name) = &expr.kind else {
        return None;
    };

    if name == "Self" {
        return locals.get(name).map(|local| local.type_.clone());
    }
    if locals.contains_key(name) {
        return None;
    }
    if let Some(type_) = BasicType::from_name(name) {
        Some(LoweredType::Basic(type_))
    } else if structs.contains_key(name) {
        Some(LoweredType::Struct(name.clone()))
    } else if enums.contains_key(name) {
        Some(LoweredType::Enum(name.clone()))
    } else {
        None
    }
}
