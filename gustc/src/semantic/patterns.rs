impl Analyzer {
    fn validate_pattern(
        &mut self,
        pattern: &Pattern,
        value_type: &Type,
        value_mutable: bool,
        payload_origin: Option<(&str, &str)>,
    ) -> Option<String> {
        match (pattern, value_type) {
            (
                Pattern::Or {
                    alternatives,
                    span,
                },
                _,
            ) => {
                self.validate_or_pattern(
                    alternatives,
                    *span,
                    value_type,
                    value_mutable,
                    payload_origin,
                );
                None
            }
            (
                Pattern::Variant {
                    enum_name: pattern_enum_name,
                    variant,
                    payload: pattern_payload,
                    span,
                },
                Type::Enum(enum_name),
            ) => {
                if enum_name != pattern_enum_name {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "pattern `{pattern_enum_name}.{variant}` does not belong to enum `{enum_name}`"
                        ),
                    ));
                    return None;
                }

                let Some(payload) = self
                    .enums
                    .get(pattern_enum_name)
                    .and_then(|enum_| enum_.variants.get(variant))
                    .cloned()
                else {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!("unknown pattern `{pattern_enum_name}.{variant}`"),
                    ));
                    return None;
                };

                match (pattern_payload, payload) {
                    (Some(pattern_payload), Some(payload)) => {
                        self.validate_pattern(
                            pattern_payload,
                            &payload,
                            value_mutable,
                            Some((pattern_enum_name, variant)),
                        );
                    }
                    (Some(_), None) => self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "unit variant `{pattern_enum_name}.{variant}` does not bind a payload"
                        ),
                    )),
                    (None, Some(payload)) => self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "`{pattern_enum_name}.{variant}` contains a `{}` value; use `{pattern_enum_name}.{variant}(value)` to bind it or `{pattern_enum_name}.{variant}(_)` to ignore it",
                            payload.name()
                        ),
                    )),
                    (None, None) => {}
                }

                Some(variant.clone())
            }
            (
                Pattern::Struct {
                    name,
                    fields,
                    has_rest,
                    span,
                },
                Type::Struct(value_name),
            ) => {
                if name != value_name {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!("pattern `{name}` does not match struct `{value_name}`"),
                    ));
                    return None;
                }

                let Some(struct_) = self.structs.get(name).cloned() else {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!("unknown struct `{name}` in match pattern"),
                    ));
                    return None;
                };

                let mut seen_fields = HashSet::new();
                for field in fields {
                    if !seen_fields.insert(field.name.clone()) {
                        self.diagnostics.push(Diagnostic::error(
                            field.span,
                            format!("duplicate field `{}` in struct pattern `{name}`", field.name),
                        ));
                        continue;
                    }

                    let Some(field_info) = struct_.fields.get(&field.name) else {
                        self.diagnostics.push(Diagnostic::error(
                            field.span,
                            format!("unknown field `{}` for struct `{name}`", field.name),
                        ));
                        continue;
                    };
                    let field_mutable = value_mutable
                        && (!field_info.internal || self.can_mutate_internal_field(name));

                    self.validate_pattern(
                        &field.pattern,
                        &field_info.type_,
                        field_mutable,
                        None,
                    );
                }

                if !has_rest {
                    let mut missing = struct_
                        .fields
                        .keys()
                        .filter(|name| !seen_fields.contains(*name))
                        .cloned()
                        .collect::<Vec<_>>();
                    missing.sort();
                    for field in missing {
                        self.diagnostics.push(Diagnostic::error(
                            *span,
                            format!(
                                "struct pattern `{name}` is missing field `{field}`; add `...` to ignore remaining fields"
                            ),
                        ));
                    }
                }

                None
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
                        self.diagnostics.push(Diagnostic::error(
                            *span,
                            format!(
                                "cannot bind mutable payload `{name}` from an immutable match value"
                            ),
                        ));
                    }
                    if let Some((enum_name, variant)) = payload_origin {
                        self.define_match_payload(
                            name,
                            *mutable,
                            value_type.clone(),
                            enum_name,
                            variant,
                            value_mutable,
                        );
                    } else {
                        self.define(name, *mutable, value_type.clone());
                    }
                }
                None
            }
            (Pattern::String { .. }, Type::Basic(BasicType::String)) => None,
            (Pattern::Bool { .. }, Type::Basic(BasicType::Bool)) => None,
            (Pattern::Number { value, span }, Type::Basic(type_)) if type_.is_integer() => {
                if !integer_pattern_literal_is_valid(value, *type_) {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "numeric match patterns for `{}` require integer literals in range",
                            type_.name()
                        ),
                    ));
                }
                None
            }
            (
                Pattern::Range {
                    start, end, span, ..
                },
                Type::Basic(type_),
            ) if type_.is_integer() => {
                if !integer_pattern_literal_is_valid(start, *type_)
                    || !integer_pattern_literal_is_valid(end, *type_)
                {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "numeric range patterns for `{}` require integer literal bounds in range",
                            type_.name()
                        ),
                    ));
                }
                None
            }
            (Pattern::Wildcard { .. }, _) => None,
            (Pattern::Variant { span, .. }, Type::Basic(BasicType::String)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    "enum patterns cannot match a `string` value",
                ));
                None
            }
            (Pattern::Struct { span, .. }, Type::Enum(enum_name)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!("struct patterns cannot match enum `{enum_name}`"),
                ));
                None
            }
            (Pattern::Struct { span, .. }, Type::Basic(type_)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!("struct patterns cannot match a `{}` value", type_.name()),
                ));
                None
            }
            (Pattern::String { span, .. }, Type::Enum(enum_name)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!("string patterns cannot match enum `{enum_name}`"),
                ));
                None
            }
            (Pattern::Number { span, .. }, Type::Enum(enum_name)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!("numeric patterns cannot match enum `{enum_name}`"),
                ));
                None
            }
            (Pattern::Range { span, .. }, Type::Enum(enum_name)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!("numeric range patterns cannot match enum `{enum_name}`"),
                ));
                None
            }
            (Pattern::Bool { span, .. }, Type::Enum(enum_name)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!("bool patterns cannot match enum `{enum_name}`"),
                ));
                None
            }
            (Pattern::Number { span, .. }, Type::Basic(BasicType::String)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    "numeric patterns cannot match a `string` value",
                ));
                None
            }
            (Pattern::Range { span, .. }, Type::Basic(BasicType::String)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    "numeric range patterns cannot match a `string` value",
                ));
                None
            }
            (Pattern::Bool { span, .. }, Type::Basic(BasicType::String)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    "bool patterns cannot match a `string` value",
                ));
                None
            }
            (Pattern::Number { span, .. }, Type::Basic(type_))
            | (Pattern::Range { span, .. }, Type::Basic(type_)) => {
                if type_.is_float() {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "numeric match patterns do not support floating-point match values, got `{}`",
                            type_.name()
                        ),
                    ));
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "numeric match patterns cannot match a `{}` value",
                            type_.name()
                        ),
                    ));
                }
                None
            }
            (Pattern::String { span, .. }, _) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "string patterns cannot match a `{}` value",
                        value_type.name()
                    ),
                ));
                None
            }
            (Pattern::Bool { span, .. }, _) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "bool patterns cannot match a `{}` value",
                        value_type.name()
                    ),
                ));
                None
            }
            (Pattern::Number { span, .. } | Pattern::Range { span, .. }, _) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "numeric match patterns cannot match a `{}` value",
                        value_type.name()
                    ),
                ));
                None
            }
            (Pattern::Variant { span, .. }, _) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    "match pattern does not apply to the matched value",
                ));
                None
            }
            (Pattern::Struct { span, .. }, _) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    "struct pattern does not apply to the matched value",
                ));
                None
            }
        }
    }

    fn validate_or_pattern(
        &mut self,
        alternatives: &[Pattern],
        span: Span,
        value_type: &Type,
        value_mutable: bool,
        payload_origin: Option<(&str, &str)>,
    ) {
        if alternatives.is_empty() {
            return;
        }

        let mut alternative_bindings = Vec::new();
        for alternative in alternatives {
            self.push_scope();
            self.validate_pattern(alternative, value_type, value_mutable, payload_origin);
            alternative_bindings.push(self.scopes.pop().unwrap_or_default());
        }

        let first_bindings = alternative_bindings
            .first()
            .expect("or-pattern should have a first alternative");
        let mut expected_names = first_bindings.keys().cloned().collect::<Vec<_>>();
        expected_names.sort();

        for bindings in alternative_bindings.iter().skip(1) {
            let mut names = bindings.keys().cloned().collect::<Vec<_>>();
            names.sort();

            for name in expected_names.iter().filter(|name| !bindings.contains_key(*name)) {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!(
                        "or-pattern alternatives must bind the same names; binding `{name}` is missing from an alternative"
                    ),
                ));
            }

            for name in names.iter().filter(|name| !first_bindings.contains_key(*name)) {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!(
                        "or-pattern alternatives must bind the same names; binding `{name}` is not present in the first alternative"
                    ),
                ));
            }
        }

        for name in &expected_names {
            let Some(expected) = first_bindings.get(name) else {
                continue;
            };
            for bindings in alternative_bindings.iter().skip(1) {
                let Some(binding) = bindings.get(name) else {
                    continue;
                };

                if expected.mutable != binding.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        format!(
                            "or-pattern binding `{name}` must use the same mutability in every alternative"
                        ),
                    ));
                }

                if !self.types_are_compatible(&expected.type_, &binding.type_)
                    || !self.types_are_compatible(&binding.type_, &expected.type_)
                {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        format!(
                            "or-pattern binding `{name}` has incompatible types: expected `{}`, got `{}`",
                            expected.type_.name(),
                            binding.type_.name()
                        ),
                    ));
                }
            }
        }

        if let Some(scope) = self.scopes.last_mut() {
            for name in expected_names {
                if let Some(binding) = first_bindings.get(&name) {
                    scope.insert(name, binding.clone());
                }
            }
        }
    }

    fn validate_match_branch_body(
        &mut self,
        body: &MatchBranchBody,
        expected_type: Option<Type>,
    ) -> Type {
        match body {
            MatchBranchBody::Expr(expr) => self.validate_expr_with_context(expr, expected_type),
            MatchBranchBody::Block(block) => {
                self.validate_block(block);
                Type::Unknown
            }
        }
    }
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
