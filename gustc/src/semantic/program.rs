impl Analyzer {
    fn validate_program(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                Item::Import(item) => self.unsupported(
                    item.span,
                    "imports are parsed but module resolution is not implemented yet",
                ),
                Item::Enum(item) => {
                    if item.variants.is_empty() {
                        self.diagnostics.push(Diagnostic::error(
                            item.span,
                            format!("enum `{}` must define at least one variant", item.name),
                        ));
                    }

                    for variant in &item.variants {
                        if let Some(type_ref) = &variant.payload {
                            self.validate_type(type_ref);
                        }
                    }

                    for member in &item.members {
                        match member {
                            StructMember::Method(method) => {
                                self.validate_function(
                                    method,
                                    Some(Type::Enum(item.name.clone())),
                                    true,
                                );
                            }
                            StructMember::StaticMethod(method) => self.validate_function(
                                method,
                                Some(Type::Enum(item.name.clone())),
                                false,
                            ),
                            StructMember::Field(_) => {}
                        }
                    }
                }
                Item::Struct(item) => {
                    for member in &item.members {
                        match member {
                            StructMember::Field(field) => {
                                self.validate_type(&field.type_ref);
                            }
                            StructMember::Method(method) => {
                                self.validate_function(
                                    method,
                                    Some(Type::Struct(item.name.clone())),
                                    true,
                                );
                            }
                            StructMember::StaticMethod(method) => self.validate_function(
                                method,
                                Some(Type::Struct(item.name.clone())),
                                false,
                            ),
                        }
                    }
                }
                Item::Trait(item) => self.validate_trait(item),
                Item::Impl(item) => self.validate_impl(item),
                Item::Function(function) => self.validate_function(function, None, false),
                Item::Extension(item) => {
                    let self_type = self.validate_type(&item.type_ref);
                    self.validate_function(&item.function, Some(self_type), !item.static_);
                }
            }
        }
    }

    fn validate_trait(&mut self, item: &TraitDecl) {
        let mut names = HashSet::new();
        for method in &item.methods {
            if !names.insert((method.static_, method.name.clone())) {
                continue;
            }

            let self_params = method
                .params
                .iter()
                .filter(|param| is_self_param(param))
                .collect::<Vec<_>>();
            if self_params.len() > 1 {
                self.diagnostics.push(Diagnostic::error(
                    self_params[1].span,
                    "a trait method can declare only one `self` receiver",
                ));
            }
            if let Some(param) = self_params.first() {
                if method.static_ {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "`self` receivers are only allowed on instance trait methods",
                    ));
                } else if !param.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "immutable `self` is implicit; remove it from the parameter list",
                    ));
                }
                if param.type_ref.is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "mutable receivers must be written `mut self` without a type annotation",
                    ));
                }
            }

            self.self_types.push(Type::Named("Self".to_string()));
            for param in &method.params {
                if is_self_param(param) {
                    continue;
                }
                if let Some(type_ref) = &param.type_ref {
                    self.validate_type(type_ref);
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        format!(
                            "trait method `{}.{}` parameters must include type annotations",
                            item.name, method.name
                        ),
                    ));
                }
            }
            if let Some(return_type) = &method.return_type {
                self.validate_type(return_type);
            }
            self.self_types.pop();
        }
    }

    fn validate_impl(&mut self, item: &ImplDecl) {
        if !self.traits.contains_key(&item.trait_ref.name) {
            self.diagnostics.push(Diagnostic::error(
                item.trait_ref.span,
                format!("unknown trait `{}`", item.trait_ref.name),
            ));
        }
        let self_type = self.validate_type(&item.type_ref);
        for member in &item.methods {
            let method = &member.function;
            let expected_return_type = method.name.as_ref().and_then(|name| {
                self.traits
                    .get(&item.trait_ref.name)
                    .and_then(|trait_| {
                        if member.static_ {
                            trait_.static_methods.get(name)
                        } else {
                            trait_.methods.get(name)
                        }
                    })
                    .map(|signature| signature_with_self_type(signature, &self_type).return_type)
            });
            self.validate_function_with_return_type(
                method,
                Some(self_type.clone()),
                !member.static_,
                expected_return_type,
            );
        }
    }

    fn validate_function(
        &mut self,
        function: &FunctionDecl,
        self_type: Option<Type>,
        has_self: bool,
    ) {
        self.validate_function_with_return_type(function, self_type, has_self, None);
    }

    fn validate_function_with_return_type(
        &mut self,
        function: &FunctionDecl,
        self_type: Option<Type>,
        has_self: bool,
        inferred_return_type: Option<Type>,
    ) {
        self.push_scope();

        if let Some(self_type) = self_type.clone() {
            self.self_types.push(self_type.clone());
            if has_self {
                self.define("self", has_mutable_receiver(function), self_type.clone());
            }
        }

        let self_params = function
            .params
            .iter()
            .filter(|param| is_self_param(param))
            .collect::<Vec<_>>();
        if has_self {
            if self_params.len() > 1 {
                self.diagnostics.push(Diagnostic::error(
                    self_params[1].span,
                    "a function can declare only one `self` receiver",
                ));
            }
            if let Some(param) = self_params.first() {
                if !param.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "immutable `self` is implicit; remove it from the parameter list",
                    ));
                }
                if param.type_ref.is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "mutable receivers must be written `mut self` without a type annotation",
                    ));
                }
            }
        } else if let Some(param) = self_params.first() {
            self.diagnostics.push(Diagnostic::error(
                param.span,
                "`self` receivers are only allowed on instance methods and extension functions",
            ));
        }

        for param in &function.params {
            if is_self_param(param) {
                continue;
            }
            let type_ = param
                .type_ref
                .as_ref()
                .map_or(Type::Unknown, |type_ref| self.validate_type(type_ref));

            self.define(&param.name, param.mutable, type_);
        }

        let return_type = function.return_type.as_ref().map_or_else(
            || inferred_return_type.unwrap_or(Type::Unknown),
            |type_ref| self.validate_type(type_ref),
        );
        self.return_types.push(return_type.clone());

        match &function.body {
            FunctionBody::Block(block) => self.validate_block(block),
            FunctionBody::Expr(expr) => {
                let value_type = self.validate_expr_with_context(expr, Some(return_type.clone()));
                self.report_type_mismatch(expr.span, return_type.clone(), value_type);
            }
        }

        self.validate_missing_return(function, return_type);
        self.return_types.pop();
        if self_type.is_some() {
            self.self_types.pop();
        }
        self.pop_scope();
    }

    fn validate_block(&mut self, block: &Block) {
        self.push_scope();

        for statement in &block.statements {
            self.validate_statement(statement);
        }

        self.pop_scope();
    }

    fn validate_statement(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Let {
                name,
                mutable,
                type_annotation,
                value,
            } => {
                let annotated_type = type_annotation
                    .as_ref()
                    .map(|type_ref| self.validate_type(type_ref));
                let value_type = if let Some(value) = value {
                    self.validate_expr_with_context(value, annotated_type.clone())
                } else {
                    if type_annotation.is_none() {
                        self.diagnostics.push(Diagnostic::error(
                            statement.span,
                            "let declarations without values must include a type annotation",
                        ));
                    } else if type_annotation
                        .as_ref()
                        .is_some_and(|type_ref| self.requires_unsupported_default(type_ref))
                    {
                        self.diagnostics.push(Diagnostic::error(
                            statement.span,
                            "default values are only supported for basic types",
                        ));
                    }

                    Type::Unknown
                };

                if let Some(annotated_type) = annotated_type.clone() {
                    self.report_type_mismatch(statement.span, annotated_type, value_type.clone());
                }

                let binding_type = annotated_type.clone().unwrap_or_else(|| value_type.clone());
                if *mutable
                    && value.as_ref().is_some_and(|value| {
                        self.requires_mutable_capability(&binding_type)
                            && !self.expr_has_mutable_capability(value)
                    })
                {
                    self.diagnostics.push(Diagnostic::error(
                        value.as_ref().map_or(statement.span, |value| value.span),
                        format!(
                            "cannot initialize mutable binding `{name}` from an immutable value; use `.clone()` to create an independent mutable object"
                        ),
                    ));
                }

                self.define(name, *mutable, annotated_type.unwrap_or(value_type));
            }
            StmtKind::Assign { target, op, value } => {
                if matches!(target.kind, ExprKind::Member { .. }) {
                    self.validate_member_assignment(statement.span, target, *op, value);
                    return;
                }

                let ExprKind::Identifier(name) = &target.kind else {
                    self.validate_expr(target);
                    self.validate_expr(value);
                    self.diagnostics.push(Diagnostic::error(
                        target.span,
                        "assignment target must be a mutable local binding",
                    ));
                    return;
                };

                let Some(binding) = self.lookup(name) else {
                    self.validate_expr(target);
                    self.validate_expr(value);
                    return;
                };

                if !binding.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        target.span,
                        format!("cannot assign to immutable binding `{name}`"),
                    ));
                }

                if op.is_none()
                    && binding.mutable
                    && self.requires_mutable_capability(&binding.type_)
                    && !self.expr_has_mutable_capability(value)
                {
                    self.diagnostics.push(Diagnostic::error(
                        value.span,
                        format!(
                            "cannot assign an immutable value to mutable binding `{name}`; use `.clone()` to create an independent mutable object"
                        ),
                    ));
                }

                let value_type = self.validate_assignment_value(
                    statement.span,
                    target,
                    *op,
                    value,
                    binding.type_.clone(),
                );
                self.report_type_mismatch(value.span, binding.type_, value_type);
            }
            StmtKind::Return { value } => {
                let expected_type = self.current_return_type();

                if let Some(value) = value {
                    let value_type =
                        self.validate_expr_with_context(value, Some(expected_type.clone()));
                    self.report_type_mismatch(value.span, expected_type, value_type);
                } else if !matches!(expected_type, Type::Unknown | Type::Void) {
                    self.diagnostics.push(Diagnostic::error(
                        statement.span,
                        "return value required for this function",
                    ));
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition_type =
                    self.validate_expr_with_context(condition, Some(Type::Basic(BasicType::Bool)));
                self.report_type_mismatch(
                    condition.span,
                    Type::Basic(BasicType::Bool),
                    condition_type,
                );
                self.validate_block(then_branch);

                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.validate_block(block),
                        ElseBranch::If(statement) => self.validate_statement(statement),
                    }
                }
            }
            StmtKind::While { condition, body } => {
                let condition_type =
                    self.validate_expr_with_context(condition, Some(Type::Basic(BasicType::Bool)));
                self.report_type_mismatch(
                    condition.span,
                    Type::Basic(BasicType::Bool),
                    condition_type,
                );
                self.loop_depth += 1;
                self.validate_block(body);
                self.loop_depth -= 1;
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        statement.span,
                        "`break` can only be used inside a loop",
                    ));
                }
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        statement.span,
                        "`continue` can only be used inside a loop",
                    ));
                }
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                let iterable_type = self.validate_expr(iterable);
                let item_type = self.for_item_type(&iterable_type);
                if item_type.is_none() && !matches!(iterable_type, Type::Unknown) {
                    self.diagnostics.push(Diagnostic::error(
                        iterable.span,
                        format!(
                            "`for` requires an `Iterator` or `Iterable`, got `{}`",
                            iterable_type.name()
                        ),
                    ));
                }
                if self.for_uses_iterator_directly(&iterable_type)
                    && !self.expr_has_mutable_capability(iterable)
                {
                    self.diagnostics.push(Diagnostic::error(
                        iterable.span,
                        "cannot advance an iterator through an immutable binding; declare it with `let mut` or iterate an `Iterable` instead",
                    ));
                }
                self.loop_depth += 1;
                self.push_scope();
                self.define(name, false, item_type.unwrap_or(Type::Unknown));

                for statement in &body.statements {
                    self.validate_statement(statement);
                }

                self.pop_scope();
                self.loop_depth -= 1;
            }
            StmtKind::Expr(expr) => {
                self.validate_expression_statement(expr);
            }
        }
    }

    fn validate_expression_statement(&mut self, expr: &Expr) {
        let ExprKind::Binary {
            left,
            op: BinaryOp::LogicalAnd | BinaryOp::LogicalOr,
            right,
        } = &expr.kind
        else {
            self.validate_expr(expr);
            return;
        };

        let expected_type = Type::Basic(BasicType::Bool);
        let left_type = self.validate_expr_with_context(left, Some(expected_type.clone()));
        self.report_type_mismatch(left.span, expected_type, left_type);
        self.validate_expression_statement(right);
    }

    fn validate_function_value_call(
        &mut self,
        expr: &Expr,
        params: &[FunctionTypeParam],
        return_type: &Type,
        args: &[Expr],
    ) -> Type {
        if args.len() != params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "function value expects {} arguments, got {}",
                    params.len(),
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return return_type.clone();
        }

        for (arg, param) in args.iter().zip(params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_.clone(), arg_type);

            if param.mutable
                && self.requires_mutable_capability(&param.type_)
                && !self.expr_has_mutable_capability(arg)
            {
                self.diagnostics.push(Diagnostic::error(
                    arg.span,
                    "function value requires a mutable argument; use `.clone()` to pass an independent mutable object",
                ));
            }
        }

        return_type.clone()
    }

    fn validate_missing_return(&mut self, function: &FunctionDecl, return_type: Type) {
        if matches!(return_type, Type::Unknown | Type::Void) {
            return;
        }

        let FunctionBody::Block(block) = &function.body else {
            return;
        };

        if block_always_returns_value(block) {
            return;
        }

        self.diagnostics.push(Diagnostic::error(
            function.span,
            "missing return value for function with explicit return type",
        ));
    }

    fn report_type_mismatch(&mut self, span: Span, expected_type: Type, value_type: Type) {
        if matches!(expected_type, Type::Unknown) || matches!(value_type, Type::Unknown) {
            return;
        }

        if !self.types_are_compatible(&expected_type, &value_type) {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "expected value of type `{}`, got `{}`",
                    expected_type.name(),
                    value_type.name()
                ),
            ));
        }
    }

}
