impl Analyzer {
    fn validate_expr(&mut self, expr: &Expr) -> Type {
        self.validate_expr_with_context(expr, None)
    }

    fn validate_expr_with_context(&mut self, expr: &Expr, expected_type: Option<Type>) -> Type {
        match &expr.kind {
            ExprKind::Identifier(name) => {
                if let Some(binding) = self.lookup(name) {
                    binding.type_
                } else if let Some(signature) = self.functions.get(name) {
                    Type::Function {
                        params: signature
                            .params
                            .iter()
                            .map(|param| FunctionTypeParam {
                                type_: param.type_.clone(),
                                mutable: param.mutable,
                            })
                            .collect(),
                        return_type: Box::new(signature.return_type.clone()),
                    }
                } else if self.values.contains(name) {
                    Type::Unknown
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("unknown name `{name}`"),
                    ));
                    Type::Unknown
                }
            }
            ExprKind::Number(value) => match expected_type {
                Some(Type::Basic(type_))
                    if type_.is_numeric()
                        && (!number_literal_is_float(value) || type_.is_float()) =>
                {
                    Type::Basic(type_)
                }
                Some(Type::Unknown) => Type::Unknown,
                Some(Type::Basic(_))
                | Some(Type::Struct(_))
                | Some(Type::Enum(_))
                | Some(Type::Trait(_))
                | Some(Type::Function { .. })
                | Some(Type::Void)
                | Some(Type::Named(_))
                | None => Type::Basic(if number_literal_is_float(value) {
                    BasicType::F64
                } else {
                    BasicType::I32
                }),
            },
            ExprKind::String(_) => Type::Basic(BasicType::String),
            ExprKind::Char(_) => Type::Basic(BasicType::Char),
            ExprKind::Bool(_) => Type::Basic(BasicType::Bool),
            ExprKind::Missing => Type::Unknown,
            ExprKind::GenericType { .. } => Type::Unknown,
            ExprKind::GenericMember { object, .. } => {
                self.validate_expr(object);
                Type::Unknown
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.validate_expr(item);
                }

                Type::Unknown
            }
            ExprKind::CollectionLiteral { items, .. } => {
                for item in items {
                    self.validate_expr(item);
                }

                Type::Unknown
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Member { object, name } = &callee.kind
                    && name == "clone"
                {
                    return self.validate_clone(expr.span, object, args);
                }

                if let ExprKind::Identifier(name) = &callee.kind {
                    return self.validate_call(expr, name, args);
                }
                if let ExprKind::Member { object, name } = &callee.kind
                    && let ExprKind::Identifier(enum_name) = &object.kind
                    && self
                        .enums
                        .get(enum_name)
                        .is_some_and(|enum_| enum_.variants.contains_key(name))
                {
                    return self.validate_variant_call(expr, enum_name, name, args);
                }
                if let ExprKind::Member { object, name } = &callee.kind
                    && let Some(type_) = self.resolve_type_expression(object)
                {
                    return self.validate_static_call(expr, type_, name, args);
                }
                if let ExprKind::Member { object, name } = &callee.kind {
                    return self.validate_method_call(expr, object, name, args);
                }

                let callee_type = self.validate_expr(callee);
                if let Type::Function {
                    params,
                    return_type,
                } = callee_type
                {
                    return self.validate_function_value_call(expr, &params, &return_type, args);
                }

                for arg in args {
                    self.validate_expr(arg);
                }

                Type::Unknown
            }
            ExprKind::Member { object, name } => {
                if let ExprKind::Identifier(enum_name) = &object.kind
                    && self.enums.contains_key(enum_name)
                {
                    self.validate_unit_variant(expr.span, enum_name, name)
                } else {
                    self.validate_member(expr.span, object, name)
                }
            }
            ExprKind::StructInit { name, fields, .. } => {
                self.validate_struct_init(expr.span, name, fields)
            }
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => self.validate_range(expr.span, start, end, *inclusive),
            ExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => {
                let operand_type =
                    self.validate_expr_with_context(operand, Some(Type::Basic(BasicType::Bool)));
                self.report_type_mismatch(
                    operand.span,
                    Type::Basic(BasicType::Bool),
                    operand_type.clone(),
                );

                if matches!(operand_type, Type::Unknown) {
                    Type::Unknown
                } else {
                    Type::Basic(BasicType::Bool)
                }
            }
            ExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } => {
                let operand_type = self.validate_expr_with_context(operand, expected_type.clone());

                if matches!(operand_type, Type::Unknown) {
                    Type::Unknown
                } else if matches!(
                    operand_type,
                    Type::Basic(type_) if type_.is_signed_numeric()
                ) {
                    operand_type
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "operator - only supports signed numeric operands, got `{}`",
                            operand_type.name()
                        ),
                    ));
                    Type::Unknown
                }
            }
            ExprKind::Binary {
                left,
                op: BinaryOp::LogicalAnd | BinaryOp::LogicalOr,
                right,
            } => {
                let expected_type = Type::Basic(BasicType::Bool);
                let left_type = self.validate_expr_with_context(left, Some(expected_type.clone()));
                let right_type =
                    self.validate_expr_with_context(right, Some(expected_type.clone()));
                self.report_type_mismatch(left.span, expected_type.clone(), left_type.clone());
                self.report_type_mismatch(right.span, expected_type, right_type.clone());

                if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
                    Type::Unknown
                } else {
                    Type::Basic(BasicType::Bool)
                }
            }
            ExprKind::Binary {
                left,
                op:
                    op @ (BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Remainder),
                right,
            } => self.validate_arithmetic(expr.span, left, *op, right, expected_type.clone()),
            ExprKind::Binary {
                left,
                op:
                    op @ (BinaryOp::BitwiseAnd
                    | BinaryOp::BitwiseOr
                    | BinaryOp::BitwiseXor
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight),
                right,
            } => self.validate_bitwise(expr.span, left, *op, right, expected_type.clone()),
            ExprKind::Binary { left, op, right } => {
                self.validate_comparison(expr.span, left, *op, right)
            }
            ExprKind::Match { value, branches } => {
                let value_type = self.validate_expr(value);
                let value_mutable = self.expr_has_mutable_capability(value);
                let mut seen = HashSet::new();
                let mut has_wildcard = false;
                let mut branch_type = None;

                for branch in branches {
                    if has_wildcard {
                        self.diagnostics.push(Diagnostic::error(
                            branch.pattern.span(),
                            "match branches after a wildcard are unreachable",
                        ));
                    }
                    self.push_scope();
                    match (&value_type, &branch.pattern) {
                        (Type::Enum(enum_name), Pattern::Variant { .. }) => {
                            if let Some(variant_name) = self.validate_pattern(
                                &branch.pattern,
                                Some(enum_name.as_str()),
                                value_mutable,
                            ) && !seen.insert(variant_name.clone())
                            {
                                self.diagnostics.push(Diagnostic::error(
                                    branch.pattern.span(),
                                    format!("duplicate match branch for variant `{variant_name}`"),
                                ));
                            }
                        }
                        (Type::Basic(BasicType::String), Pattern::String { value, span }) => {
                            if !seen.insert(value.clone()) {
                                self.diagnostics.push(Diagnostic::error(
                                    *span,
                                    format!("duplicate match branch for string `{value}`"),
                                ));
                            }
                        }
                        (Type::Basic(BasicType::I32), Pattern::Number { value, span }) => {
                            if number_literal_is_float(value) {
                                self.diagnostics.push(Diagnostic::error(
                                    *span,
                                    "numeric match patterns for `i32` require integer literals",
                                ));
                            }
                        }
                        (
                            Type::Basic(BasicType::I32),
                            Pattern::Range {
                                start, end, span, ..
                            },
                        ) => {
                            if number_literal_is_float(start) || number_literal_is_float(end) {
                                self.diagnostics.push(Diagnostic::error(
                                    *span,
                                    "numeric range patterns for `i32` require integer literal bounds",
                                ));
                            }
                        }
                        (
                            Type::Enum(_)
                            | Type::Basic(BasicType::String)
                            | Type::Basic(BasicType::I32),
                            Pattern::Wildcard { span },
                        ) => {
                            if has_wildcard {
                                self.diagnostics.push(Diagnostic::error(
                                    *span,
                                    "duplicate wildcard match branch",
                                ));
                            }
                            has_wildcard = true;
                        }
                        (Type::Enum(enum_name), Pattern::String { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                format!("string patterns cannot match enum `{enum_name}`"),
                            ));
                        }
                        (Type::Basic(BasicType::String), Pattern::Variant { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                "enum patterns cannot match a `string` value",
                            ));
                        }
                        (Type::Basic(BasicType::String), Pattern::Number { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                "numeric patterns cannot match a `string` value",
                            ));
                        }
                        (Type::Basic(BasicType::String), Pattern::Range { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                "numeric range patterns cannot match a `string` value",
                            ));
                        }
                        (Type::Enum(enum_name), Pattern::Number { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                format!("numeric patterns cannot match enum `{enum_name}`"),
                            ));
                        }
                        (Type::Enum(enum_name), Pattern::Range { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                format!("numeric range patterns cannot match enum `{enum_name}`"),
                            ));
                        }
                        (Type::Basic(type_), Pattern::Number { span, .. })
                        | (Type::Basic(type_), Pattern::Range { span, .. })
                            if *type_ != BasicType::I32 =>
                        {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                format!(
                                    "numeric match patterns currently require an `i32` match value, got `{}`",
                                    type_.name()
                                ),
                            ));
                        }
                        (Type::Unknown, _) => {
                            self.validate_pattern(&branch.pattern, None, value_mutable);
                        }
                        (_, _) => {}
                    }
                    let value_type =
                        self.validate_match_branch_body(&branch.body, expected_type.clone());
                    self.pop_scope();

                    if !matches!(value_type, Type::Unknown) {
                        if let Some(first_type) = branch_type.clone() {
                            self.report_type_mismatch(branch.body.span(), first_type, value_type);
                        } else {
                            branch_type = Some(value_type);
                        }
                    }
                }

                if let Type::Enum(enum_name) = &value_type
                    && !has_wildcard
                    && let Some(definition) = self.enums.get(enum_name)
                {
                    let mut missing = definition
                        .variants
                        .keys()
                        .filter(|name| !seen.contains(*name))
                        .cloned()
                        .collect::<Vec<_>>();
                    missing.sort();

                    if !missing.is_empty() {
                        self.diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "non-exhaustive match for enum `{enum_name}`; missing {}",
                                missing
                                    .iter()
                                    .map(|name| format!("`{name}`"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    }
                } else if value_type == Type::Basic(BasicType::String) && !has_wildcard {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        "non-exhaustive match for `string`; add a wildcard branch",
                    ));
                } else if value_type == Type::Basic(BasicType::I32) && !has_wildcard {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        "non-exhaustive match for `i32`; add a wildcard branch",
                    ));
                } else if !matches!(
                    value_type,
                    Type::Enum(_)
                        | Type::Basic(BasicType::String)
                        | Type::Basic(BasicType::I32)
                        | Type::Unknown
                ) {
                    self.diagnostics.push(Diagnostic::error(
                        value.span,
                        "match expressions require an enum, `string`, or `i32` value",
                    ));
                }

                branch_type.unwrap_or(Type::Unknown)
            }
            ExprKind::Lambda(function) => self.validate_lambda(expr.span, function, expected_type),
            ExprKind::PostfixIncrement(target) => {
                if matches!(target.kind, ExprKind::Member { .. }) {
                    return self.validate_member_increment(expr.span, target);
                }

                let ExprKind::Identifier(name) = &target.kind else {
                    self.validate_expr(target);
                    self.diagnostics.push(Diagnostic::error(
                        target.span,
                        "increment target must be a mutable local binding",
                    ));
                    return Type::Unknown;
                };

                let Some(binding) = self.lookup(name) else {
                    self.validate_expr(target);
                    return Type::Unknown;
                };

                if !binding.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("cannot mutate immutable binding `{name}`"),
                    ));
                }

                if matches!(&binding.type_, Type::Basic(type_) if type_.is_numeric()) {
                    binding.type_
                } else if matches!(binding.type_, Type::Unknown) {
                    Type::Unknown
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "operator ++ only supports numeric operands, got `{}`",
                            binding.type_.name()
                        ),
                    ));
                    Type::Unknown
                }
            }
        }
    }

    fn validate_member_assignment(
        &mut self,
        span: Span,
        target: &Expr,
        op: Option<BinaryOp>,
        value: &Expr,
    ) {
        let ExprKind::Member { object, .. } = &target.kind else {
            return;
        };
        let Some(binding_name) = mutable_member_root(object) else {
            self.validate_expr(target);
            self.validate_expr(value);
            self.diagnostics.push(Diagnostic::error(
                target.span,
                "field assignment target must be rooted in a mutable local struct binding",
            ));
            return;
        };
        let Some(binding) = self.lookup(binding_name) else {
            self.validate_expr(target);
            self.validate_expr(value);
            return;
        };

        if !binding.mutable {
            self.diagnostics.push(Diagnostic::error(
                target.span,
                format!("cannot mutate field of immutable binding `{binding_name}`"),
            ));
        }

        let field_type = self.validate_expr(target);
        if matches!(field_type, Type::Unknown) {
            self.validate_expr(value);
            return;
        }

        if op.is_none()
            && self.requires_mutable_capability(&field_type)
            && !self.expr_has_mutable_capability(value)
        {
            self.diagnostics.push(Diagnostic::error(
                value.span,
                "cannot assign an immutable value to a mutable field; use `.clone()` to create an independent mutable object",
            ));
        }

        let value_type =
            self.validate_assignment_value(span, target, op, value, field_type.clone());
        self.report_type_mismatch(value.span, field_type, value_type);
    }

    fn validate_member_increment(&mut self, span: Span, target: &Expr) -> Type {
        let ExprKind::Member { object, .. } = &target.kind else {
            return Type::Unknown;
        };
        let Some(binding_name) = mutable_member_root(object) else {
            self.validate_expr(target);
            self.diagnostics.push(Diagnostic::error(
                target.span,
                "increment target must be rooted in a mutable local struct binding",
            ));
            return Type::Unknown;
        };
        let Some(binding) = self.lookup(binding_name) else {
            self.validate_expr(target);
            return Type::Unknown;
        };

        if !binding.mutable {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("cannot mutate field of immutable binding `{binding_name}`"),
            ));
        }

        let field_type = self.validate_expr(target);
        if matches!(&field_type, Type::Basic(type_) if type_.is_numeric()) {
            field_type
        } else if matches!(field_type, Type::Unknown) {
            Type::Unknown
        } else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "operator ++ only supports numeric operands, got `{}`",
                    field_type.name()
                ),
            ));
            Type::Unknown
        }
    }

    fn resolve_type_expression(&self, expr: &Expr) -> Option<Type> {
        let ExprKind::Identifier(name) = &expr.kind else {
            return None;
        };

        if name == "Self" {
            return self.self_types.last().cloned();
        }
        if self.lookup(name).is_some() {
            return None;
        }
        if let Some(type_) = BasicType::from_name(name) {
            Some(Type::Basic(type_))
        } else if self.structs.contains_key(name) {
            Some(Type::Struct(name.clone()))
        } else if self.enums.contains_key(name) {
            Some(Type::Enum(name.clone()))
        } else if self.traits.contains_key(name) {
            Some(Type::Trait(name.clone()))
        } else if self.types.contains(name) && name != "void" {
            Some(Type::Named(name.clone()))
        } else {
            None
        }
    }

    fn validate_struct_init(&mut self, span: Span, name: &str, fields: &[StructInitField]) -> Type {
        let resolved_name = if name == "Self" {
            match self.self_types.last() {
                Some(Type::Struct(name)) => name.clone(),
                Some(type_) => {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        format!(
                            "`Self` is `{}` and cannot be initialized as a struct",
                            type_.name()
                        ),
                    ));
                    return Type::Unknown;
                }
                None => name.to_string(),
            }
        } else {
            name.to_string()
        };
        let name = resolved_name.as_str();
        let Some(definition) = self.structs.get(name).cloned() else {
            if self.is_imported_namespace_member(name) {
                for field in fields {
                    self.validate_expr(&field.value);
                }
                return Type::Unknown;
            }
            if BasicType::from_name(name).is_none() && !self.types.contains(name) {
                self.diagnostics
                    .push(Diagnostic::error(span, format!("unknown type `{name}`")));
            } else {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("`{name}` is not a struct type"),
                ));
            }

            for field in fields {
                self.validate_expr(&field.value);
            }

            return Type::Unknown;
        };

        let mut seen_fields = HashSet::new();

        for field in fields {
            if !seen_fields.insert(field.name.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    field.span,
                    format!("duplicate field `{}` in struct literal", field.name),
                ));
            }

            let Some(expected_type) = definition.fields.get(&field.name).cloned() else {
                self.diagnostics.push(Diagnostic::error(
                    field.span,
                    format!("unknown field `{}` for struct `{name}`", field.name),
                ));
                self.validate_expr(&field.value);
                continue;
            };

            let value_type =
                self.validate_expr_with_context(&field.value, Some(expected_type.clone()));
            self.report_type_mismatch(field.value.span, expected_type, value_type);
        }

        for field in definition.fields.keys() {
            if !seen_fields.contains(field) {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("missing field `{field}` in struct literal `{name}`"),
                ));
            }
        }

        Type::Struct(name.to_string())
    }

    fn validate_member(&mut self, span: Span, object: &Expr, name: &str) -> Type {
        let object_type = self.validate_expr(object);

        let struct_name = match object_type {
            Type::Struct(struct_name) => struct_name,
            Type::Unknown => return Type::Unknown,
            Type::Enum(_) => {
                self.unsupported(
                    span,
                    "direct enum payload member access is not implemented yet",
                );
                return Type::Unknown;
            }
            Type::Named(_) => return Type::Unknown,
            Type::Basic(_) | Type::Trait(_) | Type::Function { .. } | Type::Void => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    "field access requires a struct value",
                ));
                return Type::Unknown;
            }
        };

        let Some(definition) = self.structs.get(&struct_name) else {
            return Type::Unknown;
        };

        let Some(type_) = definition.fields.get(name) else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("unknown field `{name}` for struct `{struct_name}`"),
            ));
            return Type::Unknown;
        };

        type_.clone()
    }

    fn for_item_type(&self, iterable_type: &Type) -> Option<Type> {
        let trait_names = self.for_trait_names(iterable_type);

        trait_names
            .iter()
            .find_map(|trait_name| generic_trait_item_type_name(trait_name, "Iterator"))
            .or_else(|| {
                trait_names
                    .iter()
                    .find_map(|trait_name| generic_trait_item_type_name(trait_name, "Iterable"))
            })
            .map(|item_type_name| self.type_from_name(item_type_name))
    }

    fn validate_range(&mut self, span: Span, start: &Expr, end: &Expr, inclusive: bool) -> Type {
        let endpoint_type = Type::Basic(BasicType::I32);
        let start_type = self.validate_expr_with_context(start, Some(endpoint_type.clone()));
        let end_type = self.validate_expr_with_context(end, Some(endpoint_type.clone()));
        self.report_type_mismatch(start.span, endpoint_type.clone(), start_type);
        self.report_type_mismatch(end.span, endpoint_type, end_type);

        let type_name = if inclusive { "RangeInclusive" } else { "Range" };

        if let Some(name) = self.find_struct_by_source_name(type_name) {
            Type::Struct(name)
        } else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("range literals require an imported `{type_name}`"),
            ));
            Type::Unknown
        }
    }

    fn find_struct_by_source_name(&self, source_name: &str) -> Option<String> {
        self.structs
            .keys()
            .find_map(|name| (source_callable_name(name) == source_name).then(|| name.clone()))
    }

    fn for_uses_iterator_directly(&self, iterable_type: &Type) -> bool {
        self.for_trait_names(iterable_type)
            .iter()
            .any(|trait_name| generic_trait_item_type_name(trait_name, "Iterator").is_some())
    }

    fn for_trait_names(&self, iterable_type: &Type) -> Vec<String> {
        let iterable_name = iterable_type.name();
        match iterable_type {
            Type::Trait(trait_name) => vec![trait_name.clone()],
            Type::Basic(_) | Type::Struct(_) | Type::Enum(_) => self
                .trait_impls
                .iter()
                .filter_map(|(trait_name, self_type)| {
                    (self_type == &iterable_name).then(|| trait_name.clone())
                })
                .collect(),
            Type::Function { .. } | Type::Void | Type::Named(_) | Type::Unknown => Vec::new(),
        }
    }

}
