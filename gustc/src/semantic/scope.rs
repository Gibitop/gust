impl Analyzer {
    fn types_are_compatible(&self, expected_type: &Type, value_type: &Type) -> bool {
        match (expected_type, value_type) {
            (Type::Unknown, _) | (_, Type::Unknown) => true,
            (Type::Trait(trait_name), Type::Basic(_))
            | (Type::Trait(trait_name), Type::Struct(_))
            | (Type::Trait(trait_name), Type::Enum(_)) => self
                .trait_impls
                .contains(&(trait_name.clone(), value_type.name())),
            (
                Type::Function {
                    params,
                    return_type,
                },
                Type::Function {
                    params: value_params,
                    return_type: value_return_type,
                },
            ) => {
                params.len() == value_params.len()
                    && params.iter().zip(value_params).all(|(param, value_param)| {
                        param.mutable == value_param.mutable
                            && self.types_are_compatible(&param.type_, &value_param.type_)
                    })
                    && self.types_are_compatible(return_type, value_return_type)
            }
            _ => expected_type == value_type,
        }
    }

    fn type_from_name(&self, name: &str) -> Type {
        if let Some(type_) = BasicType::from_name(name) {
            Type::Basic(type_)
        } else if self.structs.contains_key(name) {
            Type::Struct(name.to_string())
        } else if self.enums.contains_key(name) {
            Type::Enum(name.to_string())
        } else if self.traits.contains_key(name) {
            Type::Trait(name.to_string())
        } else if name == "void" {
            Type::Void
        } else {
            Type::Unknown
        }
    }

    fn validate_type(&mut self, type_ref: &TypeRef) -> Type {
        if let Some(function) = &type_ref.function {
            let params = function
                .params
                .iter()
                .map(|param| FunctionTypeParam {
                    type_: self.validate_type(&param.type_ref),
                    mutable: param.mutable,
                })
                .collect();
            let return_type = Box::new(self.validate_type(&function.return_type));
            return Type::Function {
                params,
                return_type,
            };
        }

        if type_ref.name == "Self" {
            let self_type = self.self_types.last().cloned();
            if self_type.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    type_ref.span,
                    "`Self` is only available in methods and extension functions",
                ));
            }
            return self_type.unwrap_or(Type::Unknown);
        }

        if type_ref.name.starts_with("Self.") {
            return Type::Named(type_ref.name.clone());
        }

        let basic_type = BasicType::from_name(&type_ref.name);
        let imported_namespace_member = self.is_imported_namespace_member(&type_ref.name);

        if basic_type.is_none()
            && !self.types.contains(&type_ref.name)
            && !imported_namespace_member
        {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("unknown type `{}`", type_ref.name),
            ));
        }

        if !type_ref.args.is_empty() {
            self.unsupported(
                type_ref.span,
                "generic types are parsed but monomorphization is not implemented yet",
            );
        }

        for arg in &type_ref.args {
            self.validate_type(arg);
        }

        if imported_namespace_member || !type_ref.args.is_empty() {
            Type::Unknown
        } else if let Some(basic_type) = basic_type {
            Type::Basic(basic_type)
        } else if self.structs.contains_key(&type_ref.name) {
            Type::Struct(type_ref.name.clone())
        } else if self.enums.contains_key(&type_ref.name) {
            Type::Enum(type_ref.name.clone())
        } else if self.traits.contains_key(&type_ref.name) {
            Type::Trait(type_ref.name.clone())
        } else if type_ref.name == "void" {
            Type::Void
        } else if self.types.contains(&type_ref.name) {
            Type::Named(type_ref.name.clone())
        } else {
            Type::Unknown
        }
    }

    fn is_imported_namespace_member(&self, name: &str) -> bool {
        name.split_once('.')
            .is_some_and(|(namespace, _)| self.imported_namespaces.contains(namespace))
    }

    fn requires_unsupported_default(&self, type_ref: &TypeRef) -> bool {
        if type_ref.function.is_some() {
            return true;
        }

        if BasicType::from_name(&type_ref.name).is_some() {
            return !type_ref.args.is_empty();
        }

        self.structs.contains_key(&type_ref.name) || self.types.contains(&type_ref.name)
    }

    fn type_ref_without_diagnostics(&self, type_ref: Option<&TypeRef>) -> Type {
        let Some(type_ref) = type_ref else {
            return Type::Unknown;
        };

        if let Some(function) = &type_ref.function {
            return Type::Function {
                params: function
                    .params
                    .iter()
                    .map(|param| FunctionTypeParam {
                        type_: self.type_ref_without_diagnostics(Some(&param.type_ref)),
                        mutable: param.mutable,
                    })
                    .collect(),
                return_type: Box::new(
                    self.type_ref_without_diagnostics(Some(&function.return_type)),
                ),
            };
        }

        if !type_ref.args.is_empty() {
            return Type::Unknown;
        }

        if let Some(basic_type) = BasicType::from_name(&type_ref.name) {
            Type::Basic(basic_type)
        } else if self.structs.contains_key(&type_ref.name) {
            Type::Struct(type_ref.name.clone())
        } else if self.enums.contains_key(&type_ref.name) {
            Type::Enum(type_ref.name.clone())
        } else if self.traits.contains_key(&type_ref.name) {
            Type::Trait(type_ref.name.clone())
        } else if type_ref.name == "void" {
            Type::Void
        } else if self.types.contains(&type_ref.name) {
            Type::Named(type_ref.name.clone())
        } else {
            Type::Unknown
        }
    }

    fn type_ref_in_context(&self, type_ref: Option<&TypeRef>, self_type: &Type) -> Type {
        if let Some(type_ref) = type_ref
            && let Some(function) = &type_ref.function
        {
            return Type::Function {
                params: function
                    .params
                    .iter()
                    .map(|param| FunctionTypeParam {
                        type_: self.type_ref_in_context(Some(&param.type_ref), self_type),
                        mutable: param.mutable,
                    })
                    .collect(),
                return_type: Box::new(
                    self.type_ref_in_context(Some(&function.return_type), self_type),
                ),
            };
        }

        if type_ref.is_some_and(|type_ref| type_ref.name == "Self" && type_ref.args.is_empty()) {
            self_type.clone()
        } else {
            self.type_ref_without_diagnostics(type_ref)
        }
    }

    fn requires_mutable_capability(&self, type_: &Type) -> bool {
        matches!(type_, Type::Struct(_) | Type::Trait(_))
    }

    fn expr_has_mutable_capability(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Identifier(name) => self.lookup(name).is_some_and(|binding| binding.mutable),
            ExprKind::Member { object, .. } => self.expr_has_mutable_capability(object),
            ExprKind::GenericMember { object, .. } => self.expr_has_mutable_capability(object),
            ExprKind::CollectionLiteral { collection, .. } => self
                .requires_mutable_capability(&self.type_ref_without_diagnostics(Some(collection))),
            ExprKind::StructInit { name, fields, .. } => {
                let Some(definition) = self.structs.get(name) else {
                    return false;
                };

                fields.iter().all(|field| {
                    definition.fields.get(&field.name).is_none_or(|type_| {
                        !self.requires_mutable_capability(type_)
                            || self.expr_has_mutable_capability(&field.value)
                    })
                })
            }
            ExprKind::Call { callee, args } => {
                if matches!(
                    &callee.kind,
                    ExprKind::Member { name, .. } if name == "clone"
                ) {
                    return true;
                }

                let signature = match &callee.kind {
                    ExprKind::Identifier(name) => self.functions.get(name),
                    ExprKind::Member { object, name } => {
                        if let Some(type_) = self.resolve_type_expression(object) {
                            match &type_ {
                                Type::Struct(struct_name) => self
                                    .structs
                                    .get(struct_name)
                                    .and_then(|struct_| struct_.static_methods.get(name))
                                    .or_else(|| {
                                        self.static_extensions
                                            .get(&extension_name(&type_.name(), name))
                                    })
                                    .or_else(|| {
                                        self.static_trait_methods
                                            .get(&static_trait_method_name(&type_.name(), name))
                                    }),
                                Type::Enum(enum_name) => self
                                    .enums
                                    .get(enum_name)
                                    .and_then(|enum_| enum_.static_methods.get(name))
                                    .or_else(|| {
                                        self.static_extensions
                                            .get(&extension_name(&type_.name(), name))
                                    })
                                    .or_else(|| {
                                        self.static_trait_methods
                                            .get(&static_trait_method_name(&type_.name(), name))
                                    }),
                                _ => self
                                    .static_extensions
                                    .get(&extension_name(&type_.name(), name))
                                    .or_else(|| {
                                        self.static_trait_methods
                                            .get(&static_trait_method_name(&type_.name(), name))
                                    }),
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                signature.is_some_and(|signature| {
                    args.iter().zip(&signature.params).all(|(arg, param)| {
                        !self.requires_mutable_capability(&param.type_)
                            || self.expr_has_mutable_capability(arg)
                    })
                })
            }
            ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Number(_)
            | ExprKind::Bool(_)
            | ExprKind::Range { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Cast { .. }
            | ExprKind::Unary { .. } => true,
            ExprKind::Array(_)
            | ExprKind::GenericType { .. }
            | ExprKind::Lambda(_)
            | ExprKind::Match { .. }
            | ExprKind::PostfixIncrement(_)
            | ExprKind::Missing => false,
        }
    }

    fn unsupported(&mut self, span: Span, message: &'static str) {
        if self.unsupported_features.insert(message) {
            self.diagnostics.push(Diagnostic::warning(span, message));
        }
    }

    fn define(&mut self, name: &str, mutable: bool, type_: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                Binding {
                    mutable,
                    type_,
                    origin: BindingOrigin::Local,
                },
            );
        }
    }

    fn define_match_payload(
        &mut self,
        name: &str,
        mutable: bool,
        type_: Type,
        enum_name: &str,
        variant: &str,
        mutable_available: bool,
    ) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                Binding {
                    mutable,
                    type_,
                    origin: BindingOrigin::MatchPayload {
                        enum_name: enum_name.to_string(),
                        variant: variant.to_string(),
                        mutable_available,
                    },
                },
            );
        }
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding.clone());
            }
        }

        None
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn current_return_type(&self) -> Type {
        self.return_types.last().cloned().unwrap_or(Type::Unknown)
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

fn block_always_returns_value(block: &Block) -> bool {
    block.statements.iter().any(statement_always_returns_value)
}

fn statement_always_returns_value(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Return { value: Some(_) } => true,
        StmtKind::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            block_always_returns_value(then_branch)
                && match else_branch {
                    ElseBranch::Block(block) => block_always_returns_value(block),
                    ElseBranch::If(statement) => statement_always_returns_value(statement),
                }
        }
        StmtKind::Expr(expr) => expression_never_returns(expr),
        StmtKind::Let { .. }
        | StmtKind::Assign { .. }
        | StmtKind::Return { value: None }
        | StmtKind::While { .. }
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::If {
            else_branch: None, ..
        }
        | StmtKind::For { .. } => false,
    }
}

fn expression_never_returns(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Call { callee, .. } => {
            matches!(&callee.kind, ExprKind::Identifier(name) if name == "panic")
        }
        ExprKind::Match { branches, .. } => {
            !branches.is_empty()
                && branches.iter().all(|branch| match &branch.body {
                    MatchBranchBody::Expr(expr) => expression_never_returns(expr),
                    MatchBranchBody::Block(block) => block_always_returns_value(block),
                })
        }
        _ => false,
    }
}
