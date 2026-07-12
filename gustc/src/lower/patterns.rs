fn expression_has_mutable_capability(expr: &Expr, locals: &HashMap<String, LoweringLocal>) -> bool {
    match &expr.kind {
        ExprKind::Identifier(name) => locals.get(name).is_some_and(|local| local.mutable),
        ExprKind::Member { object, .. } => expression_has_mutable_capability(object, locals),
        ExprKind::GenericMember { object, .. } => expression_has_mutable_capability(object, locals),
        ExprKind::StructInit { .. }
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Range { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Unary { .. } => true,
        ExprKind::Call { callee, .. } => {
            matches!(&callee.kind, ExprKind::Member { name, .. } if name == "clone")
        }
        ExprKind::Array(_)
        | ExprKind::CollectionLiteral { .. }
        | ExprKind::GenericType { .. }
        | ExprKind::Lambda(_)
        | ExprKind::Match { .. }
        | ExprKind::PostfixIncrement(_)
        | ExprKind::Missing => false,
    }
}

fn lowered_expression_has_mutable_capability(
    expr: &LoweredExpr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
) -> bool {
    match &expr.kind {
        LoweredExprKind::Local(name) | LoweredExprKind::LocalCell(name) => {
            locals.get(name).is_some_and(|local| local.mutable)
        }
        LoweredExprKind::CapturedLocal { .. } => true,
        LoweredExprKind::FieldAccess { object, .. } => {
            lowered_expression_has_mutable_capability(object, locals, signatures, structs)
        }
        LoweredExprKind::StructLiteral { name, fields } => {
            let Some(struct_) = structs.get(name) else {
                return false;
            };

            fields.iter().all(|field| {
                struct_
                    .fields
                    .iter()
                    .find(|definition| definition.name == field.name)
                    .is_none_or(|definition| {
                        !matches!(definition.type_, LoweredType::Struct(_))
                            || lowered_expression_has_mutable_capability(
                                &field.value,
                                locals,
                                signatures,
                                structs,
                            )
                    })
            })
        }
        LoweredExprKind::Clone(_) => true,
        LoweredExprKind::TraitObject { .. } => true,
        LoweredExprKind::Call { name, args } => signatures.get(name).is_some_and(|signature| {
            matches!(signature.return_type, LoweredType::Struct(_))
                && args.iter().zip(&signature.params).all(|(arg, param)| {
                    !matches!(param.type_, LoweredType::Struct(_))
                        || lowered_expression_has_mutable_capability(
                            arg, locals, signatures, structs,
                        )
                })
        }),
        LoweredExprKind::IndirectCall { .. } | LoweredExprKind::DynamicCall { .. } => {
            matches!(expr.type_, LoweredType::Struct(_) | LoweredType::Trait(_))
        }
        LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::StringConcat(_, _)
        | LoweredExprKind::Not(_)
        | LoweredExprKind::Negate(_)
        | LoweredExprKind::Arithmetic { .. }
        | LoweredExprKind::Logical { .. }
        | LoweredExprKind::Comparison { .. }
        | LoweredExprKind::NumberToString(_)
        | LoweredExprKind::CollectionLiteral { .. }
        | LoweredExprKind::Closure { .. } => true,
        LoweredExprKind::Void
        | LoweredExprKind::PostfixIncrement(_)
        | LoweredExprKind::EnumLiteral { .. }
        | LoweredExprKind::EnumPayload { .. }
        | LoweredExprKind::MatchValue(_)
        | LoweredExprKind::Match { .. } => false,
    }
}

fn find_qualified_variant<'a>(
    enums: &'a HashMap<String, LoweredEnum>,
    enum_name: &str,
    variant_name: &str,
) -> Option<&'a LoweredVariant> {
    enums
        .get(enum_name)?
        .variants
        .iter()
        .find(|variant| variant.name == variant_name)
}

fn lower_match_pattern(
    pattern: &Pattern,
    value_type: &LoweredType,
    value_mutable: bool,
    locals: &mut HashMap<String, LoweringLocal>,
    enums: &HashMap<String, LoweredEnum>,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
    match_value_name: &str,
) -> Option<LoweredPattern> {
    let matched_value = LoweredExpr {
        type_: value_type.clone(),
        kind: LoweredExprKind::MatchValue(match_value_name.to_string()),
    };
    lower_match_pattern_with_expr(
        pattern,
        value_type,
        value_mutable,
        locals,
        enums,
        structs,
        diagnostics,
        &matched_value,
    )
}

fn lower_match_pattern_with_expr(
    pattern: &Pattern,
    value_type: &LoweredType,
    value_mutable: bool,
    locals: &mut HashMap<String, LoweringLocal>,
    enums: &HashMap<String, LoweredEnum>,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
    matched_value: &LoweredExpr,
) -> Option<LoweredPattern> {
    match (pattern, value_type) {
        (
            Pattern::Variant {
                enum_name,
                variant,
                payload,
                span,
            },
            LoweredType::Enum(value_enum_name),
        ) => {
            if enum_name != value_enum_name {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "pattern `{enum_name}.{variant}` does not belong to enum `{value_enum_name}`"
                    ),
                ));
                return None;
            }
            let Some(variant_definition) = enums
                .get(enum_name)
                .and_then(|enum_| enum_.variants.iter().find(|item| item.name == *variant))
            else {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!("unknown variant `{variant}` for enum `{enum_name}`"),
                ));
                return None;
            };

            let lowered_payload = match (payload, &variant_definition.payload) {
                (Some(payload), Some(payload_type)) => {
                    let payload_value = LoweredExpr {
                        type_: payload_type.clone(),
                        kind: LoweredExprKind::EnumPayload {
                            object: Box::new(matched_value.clone()),
                            variant: variant.clone(),
                        },
                    };
                    Some(Box::new(lower_match_pattern_with_expr(
                        payload,
                        payload_type,
                        value_mutable,
                        locals,
                        enums,
                        structs,
                        diagnostics,
                        &payload_value,
                    )?))
                }
                (Some(_), None) => {
                    diagnostics.push(Diagnostic::error(
                        *span,
                        format!("unit variant `{enum_name}.{variant}` does not bind a payload"),
                    ));
                    return None;
                }
                (None, Some(payload_type)) => {
                    diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "`{enum_name}.{variant}` contains a `{}` value; use `{enum_name}.{variant}(value)` to bind it or `{enum_name}.{variant}(_)` to ignore it",
                            payload_type.name()
                        ),
                    ));
                    return None;
                }
                (None, None) => None,
            };

            Some(LoweredPattern::Variant {
                enum_name: enum_name.clone(),
                variant: variant.clone(),
                payload: lowered_payload,
            })
        }
        (
            Pattern::Struct {
                name,
                fields,
                has_rest,
                span,
            },
            LoweredType::Struct(value_name),
        ) => {
            if name != value_name {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!("pattern `{name}` does not match struct `{value_name}`"),
                ));
                return None;
            }

            let Some(struct_) = structs.get(name) else {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!("unknown struct `{name}` in executable build"),
                ));
                return None;
            };

            let mut seen_fields = HashSet::new();
            let mut lowered_fields = Vec::new();
            for field in fields {
                if !seen_fields.insert(field.name.clone()) {
                    diagnostics.push(Diagnostic::error(
                        field.span,
                        format!("duplicate field `{}` in struct pattern `{name}`", field.name),
                    ));
                    continue;
                }

                let Some(field_definition) =
                    struct_.fields.iter().find(|item| item.name == field.name)
                else {
                    diagnostics.push(Diagnostic::error(
                        field.span,
                        format!("unknown field `{}` for struct `{name}`", field.name),
                    ));
                    continue;
                };

                let field_value = LoweredExpr {
                    type_: field_definition.type_.clone(),
                    kind: LoweredExprKind::FieldAccess {
                        object: Box::new(matched_value.clone()),
                        field: field.name.clone(),
                    },
                };
                let pattern = lower_match_pattern_with_expr(
                    &field.pattern,
                    &field_definition.type_,
                    value_mutable,
                    locals,
                    enums,
                    structs,
                    diagnostics,
                    &field_value,
                )?;
                lowered_fields.push(LoweredStructPatternField {
                    name: field.name.clone(),
                    pattern,
                });
            }

            if !has_rest {
                let mut missing = struct_
                    .fields
                    .iter()
                    .filter(|field| !seen_fields.contains(&field.name))
                    .map(|field| field.name.clone())
                    .collect::<Vec<_>>();
                missing.sort();
                for field in missing {
                    diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "struct pattern `{name}` is missing field `{field}`; add `...` to ignore remaining fields"
                        ),
                    ));
                }
            }

            Some(LoweredPattern::Struct {
                name: name.clone(),
                fields: lowered_fields,
            })
        }
        (
            Pattern::Binding {
                name,
                mutable,
                span,
            },
            _,
        ) => {
            if name != "_" {
                if *mutable && !value_mutable {
                    diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "cannot bind mutable payload `{name}` from an immutable match value in executable build"
                        ),
                    ));
                    return None;
                }
                locals.insert(
                    name.clone(),
                    LoweringLocal {
                        type_: value_type.clone(),
                        mutable: *mutable,
                        replacement: Some(matched_value.clone()),
                        captured: false,
                    },
                );
            }
            Some(LoweredPattern::Wildcard)
        }
        (Pattern::String { value, .. }, LoweredType::Basic(BasicType::String)) => {
            Some(LoweredPattern::String(value.clone()))
        }
        (Pattern::Number { span, .. }, LoweredType::Basic(BasicType::String)) => {
            diagnostics.push(Diagnostic::error(
                *span,
                "numeric patterns cannot match a `string` value in executable builds",
            ));
            None
        }
        (Pattern::Range { span, .. }, LoweredType::Basic(BasicType::String)) => {
            diagnostics.push(Diagnostic::error(
                *span,
                "numeric range patterns cannot match a `string` value in executable builds",
            ));
            None
        }
        (Pattern::Number { value, span }, LoweredType::Basic(BasicType::I32)) => {
            if number_literal_is_float(value) {
                diagnostics.push(Diagnostic::error(
                    *span,
                    "numeric match patterns for `i32` require integer literals in executable builds",
                ));
                return None;
            }
            Some(LoweredPattern::Number(value.clone()))
        }
        (
            Pattern::Range {
                start,
                end,
                inclusive,
                span,
            },
            LoweredType::Basic(BasicType::I32),
        ) => {
            if number_literal_is_float(start) || number_literal_is_float(end) {
                diagnostics.push(Diagnostic::error(
                    *span,
                    "numeric range patterns for `i32` require integer literal bounds in executable builds",
                ));
                return None;
            }
            Some(LoweredPattern::Range {
                start: start.clone(),
                end: end.clone(),
                inclusive: *inclusive,
            })
        }
        (Pattern::Wildcard { .. }, _) => Some(LoweredPattern::Wildcard),
        _ => {
            diagnostics.push(Diagnostic::error(
                pattern.span(),
                "match pattern does not apply to the matched value",
            ));
            None
        }
    }
}

fn mutable_member_root(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name),
        ExprKind::Member { object, .. } => mutable_member_root(object),
        _ => None,
    }
}

fn number_pair_contains_float(left: &Expr, right: &Expr) -> bool {
    matches!(&left.kind, ExprKind::Number(_))
        && matches!(&right.kind, ExprKind::Number(_))
        && (matches!(&left.kind, ExprKind::Number(value) if number_literal_is_float(value))
            || matches!(&right.kind, ExprKind::Number(value) if number_literal_is_float(value)))
}
