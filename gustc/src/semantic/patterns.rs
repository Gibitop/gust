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

                    let Some(field_type) = struct_.fields.get(&field.name) else {
                        self.diagnostics.push(Diagnostic::error(
                            field.span,
                            format!("unknown field `{}` for struct `{name}`", field.name),
                        ));
                        continue;
                    };

                    self.validate_pattern(
                        &field.pattern,
                        field_type,
                        value_mutable,
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

    fn pattern_fully_covers_type(&self, pattern: &Pattern, value_type: &Type) -> bool {
        match (pattern, value_type) {
            (Pattern::Wildcard { .. } | Pattern::Binding { .. }, _) => true,
            (Pattern::Or { alternatives, .. }, Type::Enum(enum_name)) => {
                let mut coverage = EnumPatternCoverage::default();
                for alternative in alternatives {
                    self.add_pattern_coverage(&mut coverage, alternative, value_type);
                }
                self.enum_coverage_is_exhaustive(&coverage, enum_name)
            }
            (Pattern::Or { alternatives, .. }, Type::Basic(BasicType::Bool)) => {
                let mut true_covered = false;
                let mut false_covered = false;
                for alternative in alternatives {
                    match alternative {
                        Pattern::Bool { value: true, .. } => true_covered = true,
                        Pattern::Bool { value: false, .. } => false_covered = true,
                        _ if self.pattern_fully_covers_type(alternative, value_type) => {
                            return true;
                        }
                        _ => {}
                    }
                }
                true_covered && false_covered
            }
            (Pattern::Or { alternatives, .. }, _) => alternatives
                .iter()
                .any(|alternative| self.pattern_fully_covers_type(alternative, value_type)),
            (
                Pattern::Struct {
                    name,
                    fields,
                    has_rest,
                    ..
                },
                Type::Struct(value_name),
            ) if name == value_name => {
                let Some(struct_) = self.structs.get(name) else {
                    return false;
                };
                let field_patterns = fields
                    .iter()
                    .map(|field| (field.name.as_str(), &field.pattern))
                    .collect::<HashMap<_, _>>();
                (*has_rest || field_patterns.len() == struct_.fields.len())
                    && field_patterns.iter().all(|(field_name, pattern)| {
                        struct_
                            .fields
                            .get(*field_name)
                            .is_some_and(|field_type| {
                                self.pattern_fully_covers_type(pattern, field_type)
                            })
                    })
            }
            (Pattern::Variant { .. }, Type::Enum(enum_name)) => {
                let mut coverage = EnumPatternCoverage::default();
                self.add_pattern_coverage(&mut coverage, pattern, value_type);
                self.enum_coverage_is_exhaustive(&coverage, enum_name)
            }
            _ => false,
        }
    }

    fn pattern_fully_covers_variant_payload(
        &self,
        pattern: &Pattern,
        value_type: &Type,
    ) -> bool {
        let (
            Pattern::Variant {
                enum_name,
                variant,
                payload,
                ..
            },
            Type::Enum(value_enum_name),
        ) = (pattern, value_type)
        else {
            return false;
        };
        if enum_name != value_enum_name {
            return false;
        }

        let Some(payload_type) = self
            .enums
            .get(enum_name)
            .and_then(|enum_| enum_.variants.get(variant))
        else {
            return false;
        };

        match (payload, payload_type) {
            (None, None) => true,
            (Some(payload), Some(payload_type)) => {
                self.pattern_fully_covers_type(payload, payload_type)
            }
            _ => false,
        }
    }

    fn pattern_fully_covered_variants(
        &self,
        pattern: &Pattern,
        value_type: &Type,
    ) -> Vec<String> {
        match pattern {
            Pattern::Or { alternatives, .. } => alternatives
                .iter()
                .flat_map(|alternative| {
                    self.pattern_fully_covered_variants(alternative, value_type)
                })
                .collect(),
            Pattern::Variant { variant, .. }
                if self.pattern_fully_covers_variant_payload(pattern, value_type) =>
            {
                vec![variant.clone()]
            }
            _ => Vec::new(),
        }
    }

    fn pattern_string_values(&self, pattern: &Pattern) -> Vec<(String, Span)> {
        match pattern {
            Pattern::Or { alternatives, .. } => alternatives
                .iter()
                .flat_map(|alternative| self.pattern_string_values(alternative))
                .collect(),
            Pattern::String { value, span } => vec![(value.clone(), *span)],
            _ => Vec::new(),
        }
    }

    fn pattern_bool_values(&self, pattern: &Pattern) -> Vec<(bool, Span)> {
        match pattern {
            Pattern::Or { alternatives, .. } => alternatives
                .iter()
                .flat_map(|alternative| self.pattern_bool_values(alternative))
                .collect(),
            Pattern::Bool { value, span } => vec![(*value, *span)],
            _ => Vec::new(),
        }
    }

    fn add_pattern_coverage(
        &self,
        coverage: &mut EnumPatternCoverage,
        pattern: &Pattern,
        value_type: &Type,
    ) {
        match (pattern, value_type) {
            (Pattern::Or { alternatives, .. }, _) => {
                for alternative in alternatives {
                    self.add_pattern_coverage(coverage, alternative, value_type);
                }
            }
            (Pattern::Wildcard { .. } | Pattern::Binding { .. }, Type::Enum(_)) => {
                coverage.wildcard = true;
            }
            (
                Pattern::Variant {
                    enum_name,
                    variant,
                    payload,
                    ..
                },
                Type::Enum(value_enum_name),
            ) if enum_name == value_enum_name => {
                let Some(payload_type) = self
                    .enums
                    .get(enum_name)
                    .and_then(|enum_| enum_.variants.get(variant))
                    .cloned()
                else {
                    return;
                };

                let variant_coverage = coverage
                    .variants
                    .entry(variant.clone())
                    .or_default();
                match (payload, payload_type) {
                    (None, None) => variant_coverage.full = true,
                    (Some(payload), Some(payload_type))
                        if self.pattern_fully_covers_type(payload, &payload_type) =>
                    {
                        variant_coverage.full = true;
                    }
                    (Some(payload), Some(Type::Enum(payload_enum_name))) => {
                        let payload_coverage = variant_coverage
                            .payload
                            .get_or_insert_with(Box::<EnumPatternCoverage>::default);
                        self.add_pattern_coverage(
                            payload_coverage,
                            payload,
                            &Type::Enum(payload_enum_name),
                        );
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn enum_coverage_is_exhaustive(
        &self,
        coverage: &EnumPatternCoverage,
        enum_name: &str,
    ) -> bool {
        if coverage.wildcard {
            return true;
        }

        let Some(definition) = self.enums.get(enum_name) else {
            return false;
        };
        definition.variants.iter().all(|(variant, payload)| {
            coverage
                .variants
                .get(variant)
                .is_some_and(|coverage| {
                    self.variant_coverage_is_exhaustive(coverage, payload.as_ref())
                })
        })
    }

    fn variant_coverage_is_exhaustive(
        &self,
        coverage: &VariantPatternCoverage,
        payload: Option<&Type>,
    ) -> bool {
        if coverage.full {
            return true;
        }

        let Some((Type::Enum(enum_name), payload_coverage)) =
            payload.zip(coverage.payload.as_deref())
        else {
            return false;
        };
        self.enum_coverage_is_exhaustive(payload_coverage, enum_name)
    }

    fn first_missing_enum_variant(
        &self,
        coverage: &EnumPatternCoverage,
        enum_name: &str,
    ) -> Option<String> {
        let definition = self.enums.get(enum_name)?;
        definition
            .variants
            .iter()
            .find(|(variant, payload)| {
                !coverage
                    .variants
                    .get(*variant)
                    .is_some_and(|coverage| {
                        self.variant_coverage_is_exhaustive(coverage, payload.as_ref())
                    })
            })
            .map(|(variant, _)| variant.clone())
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

#[derive(Default)]
struct EnumPatternCoverage {
    wildcard: bool,
    variants: HashMap<String, VariantPatternCoverage>,
}

#[derive(Default)]
struct VariantPatternCoverage {
    full: bool,
    payload: Option<Box<EnumPatternCoverage>>,
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
