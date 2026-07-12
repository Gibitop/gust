impl Analyzer {
    fn validate_pattern(
        &mut self,
        pattern: &Pattern,
        enum_name: Option<&str>,
        value_mutable: bool,
    ) -> Option<String> {
        match pattern {
            Pattern::Variant {
                enum_name: pattern_enum_name,
                variant,
                binding,
                binding_mutable,
                span,
            } => {
                if enum_name.is_some_and(|enum_name| enum_name != pattern_enum_name) {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "pattern `{pattern_enum_name}.{variant}` does not belong to enum `{}`",
                            enum_name.unwrap_or_default()
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

                match (binding, payload) {
                    (Some(binding), Some(payload)) if binding != "_" => {
                        if *binding_mutable && !value_mutable {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                format!(
                                    "cannot bind mutable payload `{binding}` from an immutable match value"
                                ),
                            ));
                        }
                        self.define_match_payload(
                            binding,
                            *binding_mutable,
                            payload,
                            pattern_enum_name,
                            variant,
                            value_mutable,
                        )
                    }
                    (Some(_), Some(_)) => {}
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
            Pattern::String { .. }
            | Pattern::Number { .. }
            | Pattern::Range { .. }
            | Pattern::Wildcard { .. } => None,
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
