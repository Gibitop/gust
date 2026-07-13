fn void_expr() -> LoweredExpr {
    LoweredExpr {
        type_: LoweredType::Void,
        kind: LoweredExprKind::Void,
    }
}

// Struct literal lowering needs local scope, callable signatures, known types, and diagnostics to
// validate each field against executable-build type information in one pass.
#[allow(clippy::too_many_arguments)]
fn lower_struct_init(
    expr: &Expr,
    name: &str,
    fields: &[StructInitField],
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let resolved_name = if name == "Self" {
        match locals.get("Self").map(|local| &local.type_) {
            Some(LoweredType::Struct(name)) => name.as_str(),
            _ => {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "`Self` does not name a struct in this executable build",
                ));
                return None;
            }
        }
    } else {
        name
    };
    let name = resolved_name;
    let Some(struct_) = structs.get(name) else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("unknown struct `{name}` in executable build"),
        ));
        return None;
    };

    let mut lowered_fields = Vec::new();
    let mut seen_fields = HashMap::new();
    let mut can_lower = true;

    for field in fields {
        if seen_fields.insert(field.name.clone(), field.span).is_some() {
            diagnostics.push(Diagnostic::error(
                field.span,
                format!("duplicate field `{}` in struct literal", field.name),
            ));
            can_lower = false;
        }

        let Some(expected_field) = struct_
            .fields
            .iter()
            .find(|expected_field| expected_field.name == field.name)
        else {
            diagnostics.push(Diagnostic::error(
                field.span,
                format!("unknown field `{}` for struct `{name}`", field.name),
            ));
            can_lower = false;
            continue;
        };

        if let Some(value) = lower_expr(
            &field.value,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            Some(expected_field.type_.clone()),
            "expected supported struct field value in executable builds",
        ) {
            lowered_fields.push(LoweredStructFieldValue {
                name: field.name.clone(),
                value,
            });
        }
    }

    for field in &struct_.fields {
        if !seen_fields.contains_key(&field.name) {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!("missing field `{}` in struct literal `{name}`", field.name),
            ));
            can_lower = false;
        }
    }

    if !can_lower || lowered_fields.len() != fields.len() {
        return None;
    }

    Some(LoweredExpr {
        type_: LoweredType::Struct(name.to_string()),
        kind: LoweredExprKind::StructLiteral {
            name: name.to_string(),
            fields: lowered_fields,
        },
    })
}

// Range literals are lowered through standard-library structs, so this helper needs the same
// expression-lowering environment plus both endpoint expressions.
#[allow(clippy::too_many_arguments)]
fn lower_range_literal(
    expr: &Expr,
    start: &Expr,
    end: &Expr,
    inclusive: bool,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let type_name = if inclusive { "RangeInclusive" } else { "Range" };
    let Some(name) = find_lowered_struct_by_source_name(type_name, structs) else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range literals require an imported `{type_name}` in executable builds"),
        ));
        return None;
    };
    let Some(struct_) = structs.get(&name) else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("unknown range struct `{name}` in executable build"),
        ));
        return None;
    };
    if struct_.fields.len() != 2 {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range struct `{name}` must declare only `start` and `end` fields"),
        ));
        return None;
    }
    let Some(start_field) = struct_.fields.iter().find(|field| field.name == "start") else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range struct `{name}` must declare a `start` field"),
        ));
        return None;
    };
    let Some(end_field) = struct_.fields.iter().find(|field| field.name == "end") else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range struct `{name}` must declare an `end` field"),
        ));
        return None;
    };

    let start = lower_expr(
        start,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        Some(start_field.type_.clone()),
        "expected supported range start in executable builds",
    )?;
    let end = lower_expr(
        end,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        Some(end_field.type_.clone()),
        "expected supported range end in executable builds",
    )?;

    Some(LoweredExpr {
        type_: LoweredType::Struct(name.clone()),
        kind: LoweredExprKind::StructLiteral {
            name,
            fields: vec![
                LoweredStructFieldValue {
                    name: "start".to_string(),
                    value: start,
                },
                LoweredStructFieldValue {
                    name: "end".to_string(),
                    value: end,
                },
            ],
        },
    })
}

// Expression lowering is the central executable-build dispatcher; keeping the shared tables,
// diagnostics, expected type, and diagnostic message explicit keeps call sites readable.
#[allow(clippy::too_many_arguments)]
fn lower_expr(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<LoweredType>,
    message: &str,
) -> Option<LoweredExpr> {
    let mut lowered = match &expr.kind {
        ExprKind::String(value) => LoweredExpr {
            type_: LoweredType::Basic(BasicType::String),
            kind: LoweredExprKind::StringLiteral(value.clone()),
        },
        ExprKind::Char(value) => LoweredExpr {
            type_: LoweredType::Basic(BasicType::Char),
            kind: LoweredExprKind::NumberLiteral(value.to_string()),
        },
        ExprKind::Bool(value) => LoweredExpr {
            type_: LoweredType::Basic(BasicType::Bool),
            kind: LoweredExprKind::BoolLiteral(*value),
        },
        ExprKind::Number(value) => {
            let type_ = if let Some(LoweredType::Basic(type_)) = expected_type.as_ref() {
                if type_.is_numeric() && (!number_literal_is_float(value) || type_.is_float()) {
                    *type_
                } else if number_literal_is_float(value) {
                    BasicType::F64
                } else {
                    BasicType::I32
                }
            } else if number_literal_is_float(value) {
                BasicType::F64
            } else {
                BasicType::I32
            };

            LoweredExpr {
                type_: LoweredType::Basic(type_),
                kind: LoweredExprKind::NumberLiteral(value.clone()),
            }
        }
        ExprKind::CollectionLiteral { items, collection } => lower_collection_literal(
            expr.span,
            items,
            collection,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
        )?,
        ExprKind::Identifier(name) if locals.contains_key(name) => locals[name]
            .replacement
            .clone()
            .unwrap_or_else(|| LoweredExpr {
                type_: locals[name].type_.clone(),
                kind: if locals[name].captured {
                    LoweredExprKind::LocalCell(name.clone())
                } else {
                    LoweredExprKind::Local(name.clone())
                },
            }),
        ExprKind::Identifier(name) if signatures.contains_key(name) => lower_function_value_expr(
            name,
            signatures,
            expected_type.clone(),
            diagnostics,
            expr.span,
        )?,
        ExprKind::Identifier(name) => {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!("unknown local `{name}` in executable build"),
            ));
            return None;
        }
        ExprKind::PostfixIncrement(target) => {
            let binding_name = match &target.kind {
                ExprKind::Identifier(name) => name,
                ExprKind::Member { object, .. } => {
                    let Some(name) = mutable_member_root(object) else {
                        diagnostics.push(Diagnostic::error(
                            target.span,
                            "increment target must be rooted in a mutable local struct binding in executable builds",
                        ));
                        return None;
                    };
                    name
                }
                _ => {
                    diagnostics.push(Diagnostic::error(
                        target.span,
                        "increment target must be a mutable local binding in executable builds",
                    ));
                    return None;
                }
            };
            let Some(local) = locals.get(binding_name) else {
                diagnostics.push(Diagnostic::error(
                    target.span,
                    format!("unknown local `{binding_name}` in executable build"),
                ));
                return None;
            };

            if !local.mutable {
                diagnostics.push(Diagnostic::error(
                    target.span,
                    format!("cannot mutate immutable binding `{binding_name}` in executable build"),
                ));
                return None;
            }

            let target = lower_expr(
                target,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                None,
                "expected supported increment target in executable builds",
            )?;

            if !matches!(&target.type_, LoweredType::Basic(type_) if type_.is_numeric()) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "operator ++ does not support values of type `{}` in executable builds",
                        target.type_.name()
                    ),
                ));
                return None;
            }

            LoweredExpr {
                type_: target.type_.clone(),
                kind: LoweredExprKind::PostfixIncrement(Box::new(target)),
            }
        }
        ExprKind::Unary {
            op: UnaryOp::Not,
            operand,
        } => {
            let operand = lower_expr(
                operand,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                Some(LoweredType::Basic(BasicType::Bool)),
                "expected supported boolean operand in executable builds",
            )?;

            LoweredExpr {
                type_: LoweredType::Basic(BasicType::Bool),
                kind: LoweredExprKind::Not(Box::new(operand)),
            }
        }
        ExprKind::Unary {
            op: UnaryOp::Negate,
            operand,
        } => {
            let operand = lower_expr(
                operand,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                expected_type.clone(),
                "expected supported signed numeric operand in executable builds",
            )?;

            if !matches!(
                operand.type_,
                LoweredType::Basic(type_) if type_.is_signed_numeric()
            ) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "operator - requires a signed numeric operand in executable builds",
                ));
                return None;
            }

            LoweredExpr {
                type_: operand.type_.clone(),
                kind: LoweredExprKind::Negate(Box::new(operand)),
            }
        }
        ExprKind::Cast { value, type_ref } => {
            let target_type = lower_value_type_ref(
                type_ref,
                structs,
                enums,
                traits,
                diagnostics,
                "unsupported cast target type in executable builds",
            )?;
            let source_context = if target_type == LoweredType::Basic(BasicType::Char)
                && matches!(&value.kind, ExprKind::Number(value) if !number_literal_is_float(value))
            {
                Some(LoweredType::Basic(BasicType::U8))
            } else {
                None
            };
            let value = lower_expr(
                value,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                source_context,
                "expected supported cast source in executable builds",
            )?;

            if !lowered_cast_is_supported(&value.type_, &target_type) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "`as` casts only support numeric primitive casts, `char` to integer casts, and `u8` to `char` in executable builds, got `{}` as `{}`",
                        value.type_.name(),
                        target_type.name()
                    ),
                ));
                return None;
            }

            LoweredExpr {
                type_: target_type.clone(),
                kind: LoweredExprKind::Cast {
                    value: Box::new(value),
                    type_: target_type,
                },
            }
        }
        ExprKind::Binary {
            left,
            op: op @ (BinaryOp::LogicalAnd | BinaryOp::LogicalOr),
            right,
        } => {
            let bool_type = LoweredType::Basic(BasicType::Bool);
            let left = lower_expr(
                left,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                Some(bool_type.clone()),
                "expected supported boolean operand in executable builds",
            )?;
            let right = lower_expr(
                right,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                Some(bool_type.clone()),
                "expected supported boolean operand in executable builds",
            )?;

            LoweredExpr {
                type_: bool_type,
                kind: LoweredExprKind::Logical {
                    left: Box::new(left),
                    op: *op,
                    right: Box::new(right),
                },
            }
        }
        ExprKind::Binary {
            left,
            op:
                op @ (BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight),
            right,
        } => {
            let is_bitwise = matches!(
                op,
                BinaryOp::BitwiseAnd
                    | BinaryOp::BitwiseOr
                    | BinaryOp::BitwiseXor
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight
            );
            let contextual_type = expected_type.as_ref().filter(|type_| {
                matches!(
                    type_,
                    LoweredType::Basic(type_) if if is_bitwise {
                        type_.is_integer()
                    } else {
                        type_.is_numeric()
                    }
                ) || *op == BinaryOp::Add && **type_ == LoweredType::Basic(BasicType::String)
            });
            let (left, right) = if let Some(type_) = contextual_type {
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            } else if !is_bitwise && number_pair_contains_float(left, right) {
                let type_ = LoweredType::Basic(BasicType::F64);
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(type_),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            } else if matches!(left.kind, ExprKind::Number(_))
                && !matches!(right.kind, ExprKind::Number(_))
            {
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                    "expected supported arithmetic operand in executable builds",
                )?;
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(right.type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            } else {
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                    "expected supported arithmetic operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(left.type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            };

            if *op == BinaryOp::Add && left.type_ == LoweredType::Basic(BasicType::String) {
                LoweredExpr {
                    type_: LoweredType::Basic(BasicType::String),
                    kind: LoweredExprKind::StringConcat(Box::new(left), Box::new(right)),
                }
            } else if matches!(
                left.type_,
                LoweredType::Basic(type_) if if is_bitwise {
                    type_.is_integer()
                } else {
                    type_.is_numeric()
                }
            ) {
                LoweredExpr {
                    type_: left.type_.clone(),
                    kind: LoweredExprKind::Arithmetic {
                        left: Box::new(left),
                        op: *op,
                        right: Box::new(right),
                    },
                }
            } else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "operator {} does not support values of type `{}` in executable builds",
                        op.symbol(),
                        left.type_.name()
                    ),
                ));
                return None;
            }
        }
        ExprKind::Binary { left, op, right } => {
            let (left, right) = if number_pair_contains_float(left, right) {
                let type_ = LoweredType::Basic(BasicType::F64);
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported comparison operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(type_),
                    "expected supported comparison operand in executable builds",
                )?;
                (left, right)
            } else if matches!(left.kind, ExprKind::Number(_))
                && !matches!(right.kind, ExprKind::Number(_))
            {
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                    "expected supported comparison operand in executable builds",
                )?;
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(right.type_.clone()),
                    "expected supported comparison operand in executable builds",
                )?;
                (left, right)
            } else {
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                    "expected supported comparison operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    Some(left.type_.clone()),
                    "expected supported comparison operand in executable builds",
                )?;
                (left, right)
            };

            let supported = match op {
                BinaryOp::Equal | BinaryOp::NotEqual => {
                    matches!(
                        &left.type_,
                        LoweredType::Basic(BasicType::String | BasicType::Char | BasicType::Bool)
                    ) || matches!(&left.type_, LoweredType::Basic(type_) if type_.is_numeric())
                }
                BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual => {
                    matches!(&left.type_, LoweredType::Basic(type_) if type_.is_numeric())
                }
                BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
                | BinaryOp::LogicalAnd
                | BinaryOp::LogicalOr => {
                    unreachable!("non-comparison operator is lowered separately")
                }
            };

            if !supported {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "operator {} does not support values of type `{}` in executable builds",
                        op.symbol(),
                        left.type_.name()
                    ),
                ));
                return None;
            }

            LoweredExpr {
                type_: LoweredType::Basic(BasicType::Bool),
                kind: LoweredExprKind::Comparison {
                    left: Box::new(left),
                    op: *op,
                    right: Box::new(right),
                },
            }
        }
        ExprKind::StructInit { name, fields, .. } => lower_struct_init(
            expr,
            name,
            fields,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
        )?,
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => lower_range_literal(
            expr,
            start,
            end,
            *inclusive,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
        )?,
        ExprKind::Member { object, name } => {
            if let ExprKind::Identifier(enum_name) = &object.kind
                && let Some(variant) = find_qualified_variant(enums, enum_name, name)
            {
                if variant.payload.is_some() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("enum variant `{enum_name}.{name}` requires a payload"),
                    ));
                    return None;
                }

                LoweredExpr {
                    type_: LoweredType::Enum(enum_name.clone()),
                    kind: LoweredExprKind::EnumLiteral {
                        enum_name: enum_name.clone(),
                        variant: name.clone(),
                        payload: None,
                    },
                }
            } else {
                let object = lower_expr(
                    object,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                    "expected supported field access object in executable builds",
                )?;

                let LoweredType::Struct(struct_name) = &object.type_ else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        "field access requires a struct value in executable builds",
                    ));
                    return None;
                };

                let Some(struct_) = structs.get(struct_name) else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("unknown struct `{struct_name}` in executable build"),
                    ));
                    return None;
                };

                let Some(field) = struct_.fields.iter().find(|field| field.name == *name) else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("unknown field `{name}` for struct `{struct_name}`"),
                    ));
                    return None;
                };

                LoweredExpr {
                    type_: field.type_.clone(),
                    kind: LoweredExprKind::FieldAccess {
                        object: Box::new(object),
                        field: name.clone(),
                    },
                }
            }
        }
        ExprKind::Call { callee, args } => {
            if let ExprKind::Member { object, name } = &callee.kind
                && name == "clone"
            {
                if !args.is_empty() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("`.clone()` expects no arguments, got {}", args.len()),
                    ));
                    return None;
                }

                let object = lower_expr(
                    object,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                    "expected supported clone source in executable builds",
                )?;

                if matches!(object.type_, LoweredType::Struct(_)) {
                    LoweredExpr {
                        type_: object.type_.clone(),
                        kind: LoweredExprKind::Clone(Box::new(object)),
                    }
                } else if object.type_ == LoweredType::Basic(BasicType::String) {
                    object
                } else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "`.clone()` is not supported for `{}` in executable builds",
                            object.type_.name()
                        ),
                    ));
                    return None;
                }
            } else if let ExprKind::Member { object, name } = &callee.kind
                && let ExprKind::Identifier(enum_name) = &object.kind
                && let Some(variant) = find_qualified_variant(enums, enum_name, name)
            {
                let expected_count = usize::from(variant.payload.is_some());

                if args.len() != expected_count {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "enum variant `{enum_name}.{name}` expects {expected_count} arguments, got {}",
                            args.len()
                        ),
                    ));
                    return None;
                }

                let payload = if let Some(type_) = &variant.payload {
                    Some(Box::new(lower_expr(
                        &args[0],
                        locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(type_.clone()),
                        "expected supported enum payload in executable builds",
                    )?))
                } else {
                    None
                };

                LoweredExpr {
                    type_: LoweredType::Enum(enum_name.clone()),
                    kind: LoweredExprKind::EnumLiteral {
                        enum_name: enum_name.clone(),
                        variant: name.clone(),
                        payload,
                    },
                }
            } else if let ExprKind::Member { object, name } = &callee.kind
                && let Some(type_) = lower_type_expression(object, locals, structs, enums)
            {
                let Some(lowered_name) = callable_static_name(&type_, name, signatures) else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "unknown static function `{name}` for type `{}` in executable build",
                            type_.name()
                        ),
                    ));
                    return None;
                };
                let Some(signature) = signatures.get(&lowered_name) else {
                    unreachable!("resolved static function must have a signature")
                };

                if args.len() != signature.params.len() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "static function `{}.{name}` expects {} arguments, got {}",
                            type_.name(),
                            signature.params.len(),
                            args.len()
                        ),
                    ));
                    return None;
                }

                let mut lowered_args = Vec::new();
                for (arg, param) in args.iter().zip(&signature.params) {
                    if let Some(arg) = lower_expr(
                        arg,
                        locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(param.type_.clone()),
                        "expected supported static function argument in executable builds",
                    ) {
                        lowered_args.push(arg);
                    }
                }
                if lowered_args.len() != args.len() {
                    return None;
                }

                LoweredExpr {
                    type_: signature.return_type.clone(),
                    kind: LoweredExprKind::Call {
                        name: lowered_name,
                        args: lowered_args,
                        location: lower_source_location(expr.span),
                    },
                }
            } else if let ExprKind::Member { object, name } = &callee.kind {
                let receiver_has_mutable_capability =
                    expression_has_mutable_capability(object, locals);
                let receiver_binding = mutable_member_root(object).map(str::to_string);
                let receiver_span = object.span;
                let object = lower_expr(
                    object,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                    "expected supported method receiver in executable builds",
                )?;
                if name == "toString"
                    && matches!(&object.type_, LoweredType::Basic(type_) if type_.is_numeric())
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `{}.toString` expects 0 arguments, got {}",
                                object.type_.name(),
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::String),
                        kind: LoweredExprKind::NumberToString(Box::new(object)),
                    });
                }

                if source_callable_name(name) == "toString"
                    && object.type_ == LoweredType::Basic(BasicType::String)
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `string.toString` expects 0 arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    return Some(object);
                }

                if matches!(object.type_, LoweredType::Basic(BasicType::String))
                    && source_callable_name(name) == "byteLen"
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `string.byteLen` expects 0 arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Usize),
                        kind: LoweredExprKind::FieldAccess {
                            object: Box::new(object),
                            field: "byte_len".to_string(),
                        },
                    });
                }

                if matches!(&object.type_, LoweredType::Struct(name) if is_string_builder_name(name))
                    && matches!(source_callable_name(name), "append" | "build")
                {
                    let source_name = source_callable_name(name);
                    let expected_count = usize::from(source_name == "append");
                    if args.len() != expected_count {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `StringBuilder.{source_name}` expects {expected_count} arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let mut lowered_args = vec![object];
                    for arg in args {
                        let expected_type = if source_name == "append" {
                            Some(LoweredType::Basic(BasicType::String))
                        } else {
                            None
                        };
                        lowered_args.push(lower_expr(
                            arg,
                            locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            expected_type,
                            "expected StringBuilder argument in executable builds",
                        )?);
                    }

                    return Some(LoweredExpr {
                        type_: if source_name == "append" {
                            LoweredType::Void
                        } else {
                            LoweredType::Basic(BasicType::String)
                        },
                        kind: LoweredExprKind::Call {
                            name: format!("intrinsic StringBuilder.{source_name}"),
                            args: lowered_args,
                            location: lower_source_location(expr.span),
                        },
                    });
                }

                if matches!(object.type_, LoweredType::Basic(BasicType::String))
                    && source_callable_name(name) == "len"
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `string.len` expects 0 arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }
                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Usize),
                        kind: LoweredExprKind::Call {
                            name: "intrinsic string.len".to_string(),
                            args: vec![object],
                            location: lower_source_location(expr.span),
                        },
                    });
                }

                if matches!(object.type_, LoweredType::Basic(BasicType::String))
                    && source_callable_name(name) == "isEmpty"
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `string.isEmpty` expects 0 arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let byte_len = LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Usize),
                        kind: LoweredExprKind::FieldAccess {
                            object: Box::new(object),
                            field: "byte_len".to_string(),
                        },
                    };
                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Bool),
                        kind: LoweredExprKind::Comparison {
                            left: Box::new(byte_len),
                            op: BinaryOp::Equal,
                            right: Box::new(LoweredExpr {
                                type_: LoweredType::Basic(BasicType::Usize),
                                kind: LoweredExprKind::NumberLiteral("0".to_string()),
                            }),
                        },
                    });
                }

                if let LoweredType::Trait(trait_name) = &object.type_
                    && !signatures.contains_key(&extension_name(&object.type_.name(), name))
                {
                    let Some(trait_) = traits.get(trait_name) else {
                        diagnostics.push(Diagnostic::error(
                            callee.span,
                            format!("unknown trait `{trait_name}` in executable build"),
                        ));
                        return None;
                    };
                    let source_name = source_callable_name(name);
                    let Some(method) = trait_
                        .methods
                        .iter()
                        .find(|method| method.name == source_name)
                    else {
                        diagnostics.push(Diagnostic::error(
                            callee.span,
                            format!(
                                "unknown method `{source_name}` for trait `{trait_name}` in executable build"
                            ),
                        ));
                        return None;
                    };

                    if method.mutable_self && !receiver_has_mutable_capability {
                        let qualified_name = format!("{trait_name}.{source_name}");
                        let message = if let Some(binding_name) = receiver_binding
                            && locals
                                .get(&binding_name)
                                .is_some_and(|local| !local.mutable)
                        {
                            format!(
                                "cannot call mutable function `{qualified_name}` through immutable binding `{binding_name}`; declare it with `let mut {binding_name}` or call the function on a mutable clone"
                            )
                        } else {
                            format!(
                                "mutable function `{qualified_name}` requires a mutable receiver; bind the value with `let mut` or call the function on a mutable clone"
                            )
                        };
                        diagnostics.push(Diagnostic::error(receiver_span, message));
                        return None;
                    }

                    if args.len() != method.params.len() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `{trait_name}.{source_name}` expects {} arguments, got {}",
                                method.params.len(),
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let mut lowered_args = Vec::new();
                    for (arg, param) in args.iter().zip(&method.params) {
                        if let Some(arg) = lower_expr(
                            arg,
                            locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            Some(param.type_.clone()),
                            "expected supported dynamic method argument in executable builds",
                        ) {
                            lowered_args.push(arg);
                        }
                    }

                    if lowered_args.len() != args.len() {
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: method.return_type.clone(),
                        kind: LoweredExprKind::DynamicCall {
                            object: Box::new(object),
                            method: source_name.to_string(),
                            args: lowered_args,
                            location: lower_source_location(expr.span),
                        },
                    });
                }

                let Some(lowered_name) = callable_method_name(&object.type_, name, signatures)
                else {
                    diagnostics.push(Diagnostic::error(
                        callee.span,
                        format!(
                            "unknown method `{name}` for type `{}` in executable build",
                            object.type_.name()
                        ),
                    ));
                    return None;
                };
                let Some(signature) = signatures.get(&lowered_name) else {
                    unreachable!("resolved method must have a signature")
                };
                let params = &signature.params[1..];

                if signature.mutable_self && !receiver_has_mutable_capability {
                    let qualified_name = format!("{}.{name}", object.type_.name());
                    let message = if let Some(binding_name) = receiver_binding
                        && locals
                            .get(&binding_name)
                            .is_some_and(|local| !local.mutable)
                    {
                        format!(
                            "cannot call mutable function `{qualified_name}` through immutable binding `{binding_name}`; declare it with `let mut {binding_name}` or call the function on a mutable clone"
                        )
                    } else {
                        format!(
                            "mutable function `{qualified_name}` requires a mutable receiver; bind the value with `let mut` or call the function on a mutable clone"
                        )
                    };
                    diagnostics.push(Diagnostic::error(receiver_span, message));
                    return None;
                }

                if args.len() != params.len() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "method `{}.{name}` expects {} arguments, got {}",
                            object.type_.name(),
                            params.len(),
                            args.len()
                        ),
                    ));
                    return None;
                }

                let mut lowered_args = vec![object];

                for (arg, param) in args.iter().zip(params) {
                    if let Some(arg) = lower_expr(
                        arg,
                        locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(param.type_.clone()),
                        "expected supported method argument in executable builds",
                    ) {
                        lowered_args.push(arg);
                    }
                }

                if lowered_args.len() != args.len() + 1 {
                    return None;
                }

                LoweredExpr {
                    type_: signature.return_type.clone(),
                    kind: LoweredExprKind::Call {
                        name: lowered_name,
                        args: lowered_args,
                        location: lower_source_location(expr.span),
                    },
                }
            } else {
                let try_indirect = !matches!(&callee.kind, ExprKind::Identifier(name) if signatures.contains_key(name));
                if try_indirect
                    && let Some(callee) = lower_expr(
                        callee,
                        locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        None,
                        "expected supported function value in executable builds",
                    )
                    && let LoweredType::Function {
                        params,
                        return_type,
                    } = &callee.type_
                {
                    if args.len() != params.len() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "function value expects {} arguments, got {}",
                                params.len(),
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let mut lowered_args = Vec::new();
                    for (arg, param) in args.iter().zip(params) {
                        if let Some(arg) = lower_expr(
                            arg,
                            locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            Some(param.type_.clone()),
                            "expected supported function value argument in executable builds",
                        ) {
                            lowered_args.push(arg);
                        }
                    }
                    if lowered_args.len() != args.len() {
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: *return_type.clone(),
                        kind: LoweredExprKind::IndirectCall {
                            callee: Box::new(callee),
                            args: lowered_args,
                            location: lower_source_location(expr.span),
                        },
                    });
                }

                let ExprKind::Identifier(name) = &callee.kind else {
                    diagnostics.push(Diagnostic::error(
                        callee.span,
                        "only direct helper function and enum variant calls are supported in executable builds",
                    ));
                    return None;
                };

                let Some(signature) = signatures.get(name) else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("unknown helper function `{name}` in executable build"),
                    ));
                    return None;
                };

                if args.len() != signature.params.len() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "function `{name}` expects {} arguments, got {}",
                            signature.params.len(),
                            args.len()
                        ),
                    ));
                    return None;
                }

                let mut lowered_args = Vec::new();

                for (arg, param) in args.iter().zip(&signature.params) {
                    if let Some(arg) = lower_expr(
                        arg,
                        locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(param.type_.clone()),
                        "expected supported function argument in executable builds",
                    ) {
                        lowered_args.push(arg);
                    }
                }

                if lowered_args.len() != args.len() {
                    return None;
                }

                LoweredExpr {
                    type_: signature.return_type.clone(),
                    kind: LoweredExprKind::Call {
                        name: name.clone(),
                        args: lowered_args,
                        location: lower_source_location(expr.span),
                    },
                }
            }
        }
        ExprKind::Lambda(function) => lower_lambda_expr(
            expr.span,
            function,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            expected_type.clone(),
        )?,
        ExprKind::Match { value, branches } => {
            let value_mutable = expression_has_mutable_capability(value, locals);
            let value = lower_expr(
                value,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                None,
                "expected supported match value in executable builds",
            )?;
            if !match_value_type_is_supported(&value.type_) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "match expressions require an enum, struct, `string`, `bool`, or integer value in executable builds",
                ));
                return None;
            }
            let mut compiled_branches = Vec::new();
            let mut result_type = None;
            let temp_name = match_temp_name(expr.span);
            let mut temp_counter = 0;

            for branch in branches {
                let mut branch_locals = locals.clone();
                let pattern = lower_match_pattern(
                    &branch.pattern,
                    &value.type_,
                    value_mutable,
                    &mut branch_locals,
                    enums,
                    structs,
                    diagnostics,
                )?;
                let guard = if let Some(guard) = &branch.guard {
                    Some(lower_expr(
                        guard,
                        &branch_locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(LoweredType::Basic(BasicType::Bool)),
                        "expected supported match guard in executable builds",
                    )?)
                } else {
                    None
                };

                let expected_branch_type = expected_type.clone().or_else(|| result_type.clone());
                let (statements, branch_value) = match &branch.body {
                    MatchBranchBody::Expr(branch_value) => (
                        Vec::new(),
                        lower_expr(
                            branch_value,
                            &branch_locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            expected_branch_type,
                            "expected supported match branch value in executable builds",
                        )?,
                    ),
                    MatchBranchBody::Block(block) => lower_match_expression_branch_block(
                        block,
                        &mut branch_locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        expected_branch_type,
                    )?,
                };
                result_type.get_or_insert_with(|| branch_value.type_.clone());
                compiled_branches.push(CompiledMatchBranch {
                    pattern,
                    guard,
                    statements,
                    value: Some(branch_value),
                });
            }

            let Some(result_type) = result_type else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "match expressions require at least one branch",
                ));
                return None;
            };

            let decision = compile_match_branches(
                compiled_branches,
                &temp_name,
                &value.type_,
                enums,
                structs,
                &mut temp_counter,
            );

            LoweredExpr {
                type_: result_type,
                kind: LoweredExprKind::Match {
                    value: Box::new(value),
                    temp_name,
                    decision: Box::new(decision),
                },
            }
        }
        _ => {
            diagnostics.push(Diagnostic::error(expr.span, message));
            return None;
        }
    };

    if let Some(expected_type) = expected_type {
        if let LoweredType::Trait(trait_name) = &expected_type
            && lowered.type_ != expected_type
            && lowered_type_implements_trait(traits, trait_name, &lowered.type_)
        {
            if !matches!(lowered.type_, LoweredType::Struct(_) | LoweredType::Enum(_)) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "only struct and enum values can be coerced to trait `{trait_name}` in executable builds"
                    ),
                ));
                return None;
            }

            lowered = LoweredExpr {
                type_: expected_type.clone(),
                kind: LoweredExprKind::TraitObject {
                    trait_name: trait_name.clone(),
                    self_type: lowered.type_.clone(),
                    value: Box::new(lowered),
                },
            };
        }

        if lowered.type_ != expected_type {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "expected value of type `{}`, got `{}`",
                    expected_type.name(),
                    lowered.type_.name()
                ),
            ));
            return None;
        }
    }

    Some(lowered)
}

// Collection literals lower through trait-provided constructor/add functions, so they need the
// expression environment and collection metadata together.
#[allow(clippy::too_many_arguments)]
fn lower_collection_literal(
    span: Span,
    items: &[Expr],
    collection: &TypeRef,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let collection = lower_value_type_ref(
        collection,
        structs,
        enums,
        traits,
        diagnostics,
        "collection literals require a concrete collection type in executable builds",
    )?;
    if !matches!(collection, LoweredType::Struct(_)) {
        diagnostics.push(Diagnostic::error(
            span,
            "collection literals require a struct type that implements `FromElements`",
        ));
        return None;
    }

    let Some((element, constructor, add)) = traits.values().find_map(|trait_| {
        let trait_head = trait_.name.split('<').next().unwrap_or(&trait_.name);
        if source_callable_name(trait_head) != "FromElements" {
            return None;
        }
        let element = trait_
            .methods
            .iter()
            .find(|method| method.name == "add" && method.mutable_self)
            .and_then(|method| method.params.first())
            .map(|param| param.type_.clone())?;
        let constructor = if trait_has_positional_type_arguments(&trait_.name) {
            qualified_static_trait_method_name(
                &trait_.name,
                &collection.name(),
                "withElementCapacity",
            )
        } else {
            static_trait_method_name(&collection.name(), "withElementCapacity")
        };
        let add = trait_impl_method_name(trait_, &collection, "add")?;
        (signatures.contains_key(&constructor) && signatures.contains_key(&add)).then_some((
            element,
            constructor,
            add,
        ))
    }) else {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "collection type `{}` must implement `FromElements` to use a collection literal",
                collection.name()
            ),
        ));
        return None;
    };

    let mut lowered_items = Vec::new();
    for item in items {
        lowered_items.push(lower_expr(
            item,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            Some(element.clone()),
            "collection literal elements must match the collection element type",
        )?);
    }

    Some(LoweredExpr {
        type_: collection,
        kind: LoweredExprKind::CollectionLiteral {
            constructor,
            add,
            items: lowered_items,
            location: lower_source_location(span),
        },
    })
}

fn lowered_type_implements_trait(
    traits: &HashMap<String, LoweredTrait>,
    trait_name: &str,
    type_: &LoweredType,
) -> bool {
    traits
        .get(trait_name)
        .is_some_and(|trait_| trait_.impls.iter().any(|impl_| impl_.self_type == *type_))
}

// Lambda lowering bridges outer locals, inferred function type, body lowering, and closure state;
// those inputs are intentionally explicit because captures depend on the caller's local scope.
#[allow(clippy::too_many_arguments)]
fn lower_lambda_expr(
    span: Span,
    function: &FunctionDecl,
    outer_locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<LoweredType>,
) -> Option<LoweredExpr> {
    let function_type = expected_type.or_else(|| {
        infer_lambda_function_type(
            span,
            function,
            outer_locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
        )
    })?;
    let LoweredType::Function {
        params: function_params,
        return_type,
    } = &function_type
    else {
        diagnostics.push(Diagnostic::error(
            span,
            "lambda expressions require a function type context in executable builds",
        ));
        return None;
    };
    let function_params = function_params.clone();
    let return_type = return_type.clone();

    if function.params.len() != function_params.len() {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "lambda expects {} parameters from context, got {}",
                function_params.len(),
                function.params.len()
            ),
        ));
        return None;
    }

    let name = CLOSURE_LOWERING.with(|state| {
        let mut state = state.borrow_mut();
        let name = format!("lambda{}", state.next_closure_id);
        state.next_closure_id += 1;
        name
    });

    let captures = outer_locals
        .iter()
        .filter(|(_, local)| local.captured)
        .map(|(name, local)| LoweredClosureCapture {
            name: name.clone(),
            type_: local.type_.clone(),
        })
        .collect::<Vec<_>>();

    let mut locals = HashMap::new();
    for capture in &captures {
        locals.insert(
            capture.name.clone(),
            LoweringLocal {
                type_: capture.type_.clone(),
                mutable: true,
                replacement: Some(LoweredExpr {
                    type_: capture.type_.clone(),
                    kind: LoweredExprKind::CapturedLocal {
                        env_name: "env".to_string(),
                        name: capture.name.clone(),
                    },
                }),
                captured: false,
            },
        );
    }

    let mut params = Vec::new();
    for (param, param_type) in function.params.iter().zip(function_params) {
        locals.insert(
            param.name.clone(),
            LoweringLocal {
                type_: param_type.type_.clone(),
                mutable: param.mutable,
                replacement: None,
                captured: false,
            },
        );
        params.push(LoweredParam {
            name: param.name.clone(),
            type_: param_type.type_.clone(),
        });
    }

    let mut statements = Vec::new();
    let mut return_value = None;
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
                Some(*return_type.clone()),
                "expected supported lambda return value in executable builds",
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
                                Some(*return_type.clone()),
                                "expected supported lambda return value in executable builds",
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
                            Some(return_type.as_ref()),
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
                            Some(return_type.as_ref()),
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
                            Some(return_type.as_ref()),
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
                        Some(return_type.as_ref()),
                    )),
                }
            }
        }
    }

    let return_value = return_value.unwrap_or_else(void_expr);
    CLOSURE_LOWERING.with(|state| {
        state
            .borrow_mut()
            .closure_functions
            .push(LoweredClosureFunction {
                name: name.clone(),
                captures: captures.clone(),
                params,
                return_type: *return_type.clone(),
                statements,
                return_value,
            });
    });

    Some(LoweredExpr {
        type_: function_type,
        kind: LoweredExprKind::Closure { name, captures },
    })
}

// Inferring a lambda's function type needs the outer local scope and all shared type/function
// tables, matching the later lambda body lowering path.
#[allow(clippy::too_many_arguments)]
fn infer_lambda_function_type(
    span: Span,
    function: &FunctionDecl,
    outer_locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredType> {
    let self_type = outer_locals.get("Self").map(|local| &local.type_);
    let mut locals = outer_locals
        .iter()
        .map(|(name, local)| (name.clone(), local.type_.clone()))
        .collect::<HashMap<_, _>>();
    let mut params = Vec::new();

    for param in &function.params {
        let Some(type_ref) = &param.type_ref else {
            diagnostics.push(Diagnostic::error(
                param.span,
                "lambda parameters must include type annotations when no function type context is available",
            ));
            return None;
        };
        let type_ = lower_value_type_ref_in_context(
            type_ref,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "lambda parameter types must be supported in executable builds",
        )?;
        locals.insert(param.name.clone(), type_.clone());
        params.push(LoweredFunctionTypeParam {
            type_,
            mutable: param.mutable,
        });
    }

    let return_type = if let Some(type_ref) = &function.return_type {
        lower_value_type_ref_in_context(
            type_ref,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "lambda return types must be supported in executable builds",
        )?
    } else {
        match &function.body {
            FunctionBody::Expr(expr) => {
                let Some(return_type) =
                    infer_expr_type(expr, &locals, signatures, structs, enums, traits)
                else {
                    diagnostics.push(Diagnostic::error(
                        span,
                        "could not infer lambda return type; add a function type annotation",
                    ));
                    return None;
                };
                return_type
            }
            FunctionBody::Block(block) => {
                let mut return_type = None;
                let mut has_unresolved_value_return = false;
                if let Err(conflict) = infer_block_return_types(
                    block,
                    &mut locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    &mut return_type,
                    &mut has_unresolved_value_return,
                ) {
                    diagnostics.push(Diagnostic::error(
                        conflict.span,
                        format!(
                            "lambda has multiple return types (`{}` and `{}`); inferred return types must be consistent",
                            conflict.first.name(),
                            conflict.second.name()
                        ),
                    ));
                    return None;
                }
                if has_unresolved_value_return {
                    diagnostics.push(Diagnostic::error(
                        span,
                        "could not infer lambda return type; add a function type annotation",
                    ));
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

fn lowered_cast_is_supported(source_type: &LoweredType, target_type: &LoweredType) -> bool {
    match (source_type, target_type) {
        (LoweredType::Basic(source), LoweredType::Basic(target)) => {
            (source.is_numeric() && target.is_numeric())
                || (*source == BasicType::Char && target.is_integer())
                || (*source == BasicType::U8 && *target == BasicType::Char)
        }
        _ => false,
    }
}
