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
            (Pattern::Number { value, span }, Type::Basic(BasicType::I32)) => {
                if number_literal_is_float(value) {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        "numeric match patterns for `i32` require integer literals",
                    ));
                }
                None
            }
            (
                Pattern::Range {
                    start, end, span, ..
                },
                Type::Basic(BasicType::I32),
            ) => {
                if number_literal_is_float(start) || number_literal_is_float(end) {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        "numeric range patterns for `i32` require integer literal bounds",
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
            (Pattern::Number { span, .. }, Type::Basic(type_))
            | (Pattern::Range { span, .. }, Type::Basic(type_)) => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "numeric match patterns currently require an `i32` match value, got `{}`",
                        type_.name()
                    ),
                ));
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
        }
    }

    fn pattern_fully_covers_type(&self, pattern: &Pattern, value_type: &Type) -> bool {
        match (pattern, value_type) {
            (Pattern::Wildcard { .. } | Pattern::Binding { .. }, _) => true,
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

    fn add_pattern_coverage(
        &self,
        coverage: &mut EnumPatternCoverage,
        pattern: &Pattern,
        value_type: &Type,
    ) {
        match (pattern, value_type) {
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
