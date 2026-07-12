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
        | ExprKind::Cast { .. }
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
        | LoweredExprKind::Cast { .. }
        | LoweredExprKind::NumberToString(_)
        | LoweredExprKind::CollectionLiteral { .. }
        | LoweredExprKind::Closure { .. } => true,
        LoweredExprKind::Void
        | LoweredExprKind::PostfixIncrement(_)
        | LoweredExprKind::EnumLiteral { .. }
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
) -> Option<LoweredPattern> {
    lower_match_pattern_with_expr(
        pattern,
        value_type,
        value_mutable,
        locals,
        enums,
        structs,
        diagnostics,
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
) -> Option<LoweredPattern> {
    match (pattern, value_type) {
        (
            Pattern::Or {
                alternatives,
                span,
            },
            _,
        ) => lower_or_match_pattern(
            alternatives,
            *span,
            value_type,
            value_mutable,
            locals,
            enums,
            structs,
            diagnostics,
        ),
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
                (Some(payload), Some(payload_type)) => Some(Box::new(lower_match_pattern_with_expr(
                    payload,
                    payload_type,
                    value_mutable,
                    locals,
                    enums,
                    structs,
                    diagnostics,
                )?)),
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

                let pattern = lower_match_pattern_with_expr(
                    &field.pattern,
                    &field_definition.type_,
                    value_mutable,
                    locals,
                    enums,
                    structs,
                    diagnostics,
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
            if name == "_" {
                return Some(LoweredPattern::Wildcard);
            }
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
                    replacement: None,
                    captured: false,
                },
            );
            Some(LoweredPattern::Binding {
                name: name.clone(),
            })
        }
        (Pattern::String { value, .. }, LoweredType::Basic(BasicType::String)) => {
            Some(LoweredPattern::String(value.clone()))
        }
        (Pattern::Bool { value, .. }, LoweredType::Basic(BasicType::Bool)) => {
            Some(LoweredPattern::Bool(*value))
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
        (Pattern::Number { value, span }, LoweredType::Basic(type_)) if type_.is_integer() => {
            if !integer_pattern_literal_is_valid(value, *type_) {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "numeric match patterns for `{}` require integer literals in range for executable builds",
                        type_.name()
                    ),
                ));
                return None;
            }
            Some(LoweredPattern::Number {
                value: value.clone(),
                type_: *type_,
            })
        }
        (
            Pattern::Range {
                start,
                end,
                inclusive,
                span,
            },
            LoweredType::Basic(type_),
        ) if type_.is_integer() => {
            if !integer_pattern_literal_is_valid(start, *type_)
                || !integer_pattern_literal_is_valid(end, *type_)
            {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "numeric range patterns for `{}` require integer literal bounds in range for executable builds",
                        type_.name()
                    ),
                ));
                return None;
            }
            Some(LoweredPattern::Range {
                start: start.clone(),
                end: end.clone(),
                inclusive: *inclusive,
                type_: *type_,
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

#[allow(clippy::too_many_arguments)]
fn lower_or_match_pattern(
    alternatives: &[Pattern],
    span: Span,
    value_type: &LoweredType,
    value_mutable: bool,
    locals: &mut HashMap<String, LoweringLocal>,
    enums: &HashMap<String, LoweredEnum>,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredPattern> {
    let mut lowered_alternatives = Vec::new();
    let mut alternative_locals = Vec::new();

    for alternative in alternatives {
        let mut locals_for_alternative = locals.clone();
        let lowered = lower_match_pattern_with_expr(
            alternative,
            value_type,
            value_mutable,
            &mut locals_for_alternative,
            enums,
            structs,
            diagnostics,
        )?;
        lowered_alternatives.push(lowered);
        alternative_locals.push(locals_for_alternative);
    }

    let mut binding_names = alternative_locals
        .first()
        .map(|bindings| changed_local_names(locals, bindings))
        .unwrap_or_default();
    binding_names.sort();

    for bindings in alternative_locals.iter().skip(1) {
        let mut names = changed_local_names(locals, bindings);
        names.sort();
        if names != binding_names {
            diagnostics.push(Diagnostic::error(
                span,
                "or-pattern alternatives must bind the same names in executable builds",
            ));
            return None;
        }
    }

    for name in binding_names {
        let bindings = alternative_locals
            .iter()
            .filter_map(|locals| locals.get(&name))
            .collect::<Vec<_>>();
        let Some(first) = bindings.first().copied() else {
            continue;
        };
        if bindings
            .iter()
            .any(|binding| binding.type_ != first.type_ || binding.mutable != first.mutable)
        {
            diagnostics.push(Diagnostic::error(
                span,
                format!("or-pattern binding `{name}` is inconsistent in executable builds"),
            ));
            return None;
        }

        if first.mutable {
            diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "mutable or-pattern binding `{name}` is not supported in executable builds"
                ),
            ));
            return None;
        }

        locals.insert(
            name,
            LoweringLocal {
                type_: first.type_.clone(),
                mutable: first.mutable,
                replacement: None,
                captured: first.captured,
            },
        );
    }

    Some(LoweredPattern::Or(lowered_alternatives))
}

fn changed_local_names(
    base: &HashMap<String, LoweringLocal>,
    locals: &HashMap<String, LoweringLocal>,
) -> Vec<String> {
    locals
        .iter()
        .filter(|(name, local)| base.get(*name) != Some(*local))
        .map(|(name, _)| name.clone())
        .collect()
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

fn match_value_type_is_supported(type_: &LoweredType) -> bool {
    matches!(
        type_,
        LoweredType::Enum(_)
            | LoweredType::Struct(_)
            | LoweredType::Basic(BasicType::String | BasicType::Bool)
    ) || matches!(type_, LoweredType::Basic(type_) if type_.is_integer())
}

fn integer_pattern_literal_is_valid(value: &str, type_: BasicType) -> bool {
    if number_literal_is_float(value) {
        return false;
    }
    let Ok(value) = value.parse::<u128>() else {
        return false;
    };
    integer_type_max(type_).is_some_and(|max| value <= max)
}

fn integer_type_max(type_: BasicType) -> Option<u128> {
    match type_ {
        BasicType::U8 => Some(u8::MAX as u128),
        BasicType::U16 => Some(u16::MAX as u128),
        BasicType::U32 => Some(u32::MAX as u128),
        BasicType::U64 => Some(u64::MAX as u128),
        BasicType::U128 => Some(u128::MAX),
        BasicType::Usize => Some(u64::MAX as u128),
        BasicType::I8 => Some(i8::MAX as u128),
        BasicType::I16 => Some(i16::MAX as u128),
        BasicType::I32 => Some(i32::MAX as u128),
        BasicType::I64 => Some(i64::MAX as u128),
        BasicType::I128 => Some(i128::MAX as u128),
        _ => None,
    }
}
