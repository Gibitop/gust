impl Monomorphizer {
    fn validate_associated_types(&mut self, program: &Program) {
        for trait_ in self.trait_declarations.values().cloned().collect::<Vec<_>>() {
            let mut names = HashSet::new();
            for associated_type in &trait_.associated_types {
                if !names.insert(associated_type.name.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        associated_type.span,
                        format!(
                            "duplicate associated type `{}` in trait `{}`",
                            associated_type.name, trait_.name
                        ),
                    ));
                }
                self.validate_associated_type_params(
                    &trait_.name,
                    associated_type,
                );
            }
            for method in &trait_.methods {
                if method.static_ && method.body.is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        method.span,
                        "provided trait methods must be instance methods",
                    ));
                }
            }
        }

        for item in &program.items {
            let Item::Impl(impl_) = item else {
                continue;
            };
            let Some(trait_) = self.trait_declarations.get(&impl_.trait_ref.name).cloned() else {
                continue;
            };
            let declared = trait_
                .associated_types
                .iter()
                .map(|associated_type| associated_type.name.as_str())
                .collect::<HashSet<_>>();
            let mut defined = HashSet::new();
            for associated_type in &impl_.associated_types {
                if !defined.insert(associated_type.name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        associated_type.span,
                        format!(
                            "duplicate definition of associated type `{}.{}` for `{}`",
                            trait_.name,
                            associated_type.name,
                            type_name(&impl_.type_ref)
                        ),
                    ));
                } else if !declared.contains(associated_type.name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        associated_type.span,
                        format!(
                            "trait `{}` does not declare associated type `{}` for impl on `{}`",
                            trait_.name,
                            associated_type.name,
                            type_name(&impl_.type_ref)
                        ),
                    ));
                }
                if let Some(declaration) = trait_
                    .associated_types
                    .iter()
                    .find(|declaration| declaration.name == associated_type.name)
                {
                    if declaration.type_params.len() != associated_type.type_params.len() {
                        self.diagnostics.push(Diagnostic::error(
                            associated_type.span,
                            format!(
                                "associated type `{}.{}` expects {} type arguments, got {}",
                                trait_.name,
                                associated_type.name,
                                declaration.type_params.len(),
                                associated_type.type_params.len()
                            ),
                        ));
                    }
                }
                self.validate_associated_type_definition_params(
                    &trait_.name,
                    associated_type,
                );
            }
            for associated_type in &trait_.associated_types {
                if !defined.contains(associated_type.name.as_str())
                    && associated_type.default.is_none()
                {
                    self.diagnostics.push(Diagnostic::error(
                        impl_.span,
                        format!(
                            "missing definition of associated type `{}.{}` for impl on `{}`",
                            trait_.name,
                            associated_type.name,
                            type_name(&impl_.type_ref)
                        ),
                    ));
                }
            }
        }

        for item in &program.items {
            match item {
                Item::Struct(item) => {
                    for member in &item.members {
                        match member {
                            StructMember::Field(field) => {
                                self.validate_trait_object_type(&field.type_ref)
                            }
                            StructMember::Method(function)
                            | StructMember::StaticMethod(function) => {
                                self.validate_function_associated_types(function)
                            }
                        }
                    }
                }
                Item::Enum(item) => {
                    for variant in &item.variants {
                        if let Some(payload) = &variant.payload {
                            self.validate_trait_object_type(payload);
                        }
                    }
                    for member in &item.members {
                        if let StructMember::Method(function)
                        | StructMember::StaticMethod(function) = member
                        {
                            self.validate_function_associated_types(function);
                        }
                    }
                }
                Item::Trait(item) => {
                    for method in &item.methods {
                        for param in &method.params {
                            if let Some(type_ref) = &param.type_ref {
                                self.validate_trait_object_type(type_ref);
                            }
                        }
                        if let Some(return_type) = &method.return_type {
                            self.validate_trait_object_type(return_type);
                        }
                    }
                }
                Item::Impl(item) => {
                    for member in &item.methods {
                        self.validate_function_associated_types(&member.function);
                    }
                }
                Item::Extension(item) => {
                    self.validate_function_associated_types(&item.function)
                }
                Item::Function(function) => {
                    self.validate_function_associated_types(function)
                }
                Item::Import(_) => {}
            }
        }
    }

    fn validate_associated_type_params(
        &mut self,
        trait_name: &str,
        associated_type: &crate::ast::AssociatedTypeDecl,
    ) {
        let mut names = HashSet::new();
        for name in &associated_type.type_params {
            if !names.insert(name) {
                self.diagnostics.push(Diagnostic::error(
                    associated_type.span,
                    format!(
                        "duplicate type parameter `{name}` in associated type `{}.{}`",
                        trait_name, associated_type.name
                    ),
                ));
            }
        }
    }

    fn validate_associated_type_definition_params(
        &mut self,
        trait_name: &str,
        associated_type: &crate::ast::AssociatedTypeDef,
    ) {
        let mut names = HashSet::new();
        for name in &associated_type.type_params {
            if !names.insert(name) {
                self.diagnostics.push(Diagnostic::error(
                    associated_type.span,
                    format!(
                        "duplicate type parameter `{name}` in associated type definition `{}.{}`",
                        trait_name, associated_type.name
                    ),
                ));
            }
        }
    }

    fn validate_function_associated_types(&mut self, function: &FunctionDecl) {
        for bound in &function.type_param_bounds {
            self.validate_associated_bindings(&bound.trait_ref);
            for arg in &bound.trait_ref.args {
                self.validate_trait_object_type(arg);
            }
            for binding in &bound.trait_ref.bindings {
                self.validate_trait_object_type(&binding.type_ref);
            }
        }
        for param in &function.params {
            if let Some(type_ref) = &param.type_ref {
                self.validate_trait_object_type(type_ref);
                self.validate_bounded_projection(
                    type_ref,
                    &function.type_params,
                    &function.type_param_bounds,
                );
            }
        }
        if let Some(return_type) = &function.return_type {
            self.validate_trait_object_type(return_type);
            self.validate_bounded_projection(
                return_type,
                &function.type_params,
                &function.type_param_bounds,
            );
        }
        self.validate_function_body_trait_objects(&function.body);
    }

    fn validate_trait_object_type(&mut self, type_ref: &TypeRef) {
        self.validate_associated_bindings(type_ref);
        if let Some(trait_) = self.trait_declarations.get(&type_ref.name) {
            let required = trait_
                .associated_types
                .iter()
                .filter(|associated_type| {
                    associated_type.type_params.is_empty()
                        && trait_.methods.iter().any(|method| {
                        method
                            .params
                            .iter()
                            .filter_map(|param| param.type_ref.as_ref())
                            .any(|type_ref| {
                                type_ref_contains_name(
                                    type_ref,
                                    &format!("Self.{}", associated_type.name),
                                )
                            })
                            || method.return_type.as_ref().is_some_and(|type_ref| {
                                type_ref_contains_name(
                                    type_ref,
                                    &format!("Self.{}", associated_type.name),
                                )
                            })
                    })
                })
                .map(|associated_type| associated_type.name.as_str())
                .collect::<Vec<_>>();
            let bound = type_ref
                .bindings
                .iter()
                .map(|binding| binding.name.as_str())
                .collect::<HashSet<_>>();
            if let Some(missing) = required.iter().find(|name| !bound.contains(**name)) {
                self.diagnostics.push(Diagnostic::error(
                    type_ref.span,
                    format!(
                        "trait-typed value `{}` must bind associated type `{}.{}` to determine its method signatures",
                        type_name(type_ref), trait_.name, missing
                    ),
                ));
            }
        }
        for arg in &type_ref.args {
            self.validate_trait_object_type(arg);
        }
        for binding in &type_ref.bindings {
            self.validate_trait_object_type(&binding.type_ref);
        }
        if let Some(function) = &type_ref.function {
            for param in &function.params {
                self.validate_trait_object_type(&param.type_ref);
            }
            self.validate_trait_object_type(&function.return_type);
        }
    }

    fn validate_bounded_projection(
        &mut self,
        type_ref: &TypeRef,
        type_params: &[String],
        bounds: &[TypeParamBound],
    ) {
        if type_ref.bindings.is_empty()
            && let Some((receiver, associated_type)) = type_ref.name.rsplit_once('.')
            && type_params.iter().any(|param| param == receiver)
        {
            let declaring_traits = bounds
                .iter()
                .filter(|bound| bound.param == receiver)
                .filter_map(|bound| self.trait_declarations.get(&bound.trait_ref.name))
                .filter(|trait_| {
                    trait_
                        .associated_types
                        .iter()
                        .any(|decl| decl.name == associated_type)
                })
                .map(|trait_| trait_.name.as_str())
                .collect::<Vec<_>>();
            let message = if declaring_traits.is_empty() {
                Some(format!(
                    "cannot resolve associated type projection `{}.{}`: type parameter `{}` has no bound declaring `{}`",
                    receiver, associated_type, receiver, associated_type
                ))
            } else if declaring_traits.len() > 1 {
                Some(format!(
                    "ambiguous associated type projection `{}.{}`: bounds `{}` all declare `{}`",
                    receiver,
                    associated_type,
                    declaring_traits.join("` and `"),
                    associated_type
                ))
            } else {
                None
            };
            if let Some(message) = message {
                self.diagnostics
                    .push(Diagnostic::error(type_ref.span, message));
            }
        }
        for arg in &type_ref.args {
            self.validate_bounded_projection(arg, type_params, bounds);
        }
        for binding in &type_ref.bindings {
            self.validate_bounded_projection(&binding.type_ref, type_params, bounds);
        }
        if let Some(function) = &type_ref.function {
            for param in &function.params {
                self.validate_bounded_projection(&param.type_ref, type_params, bounds);
            }
            self.validate_bounded_projection(&function.return_type, type_params, bounds);
        }
    }

    fn validate_function_body_trait_objects(&mut self, body: &FunctionBody) {
        match body {
            FunctionBody::Block(block) => self.validate_block_trait_objects(block),
            FunctionBody::Expr(expr) => self.validate_expr_trait_objects(expr),
        }
    }

    fn validate_block_trait_objects(&mut self, block: &Block) {
        for statement in &block.statements {
            match &statement.kind {
                StmtKind::Let {
                    type_annotation,
                    value,
                    ..
                } => {
                    if let Some(type_ref) = type_annotation {
                        self.validate_trait_object_type(type_ref);
                    }
                    if let Some(value) = value {
                        self.validate_expr_trait_objects(value);
                    }
                }
                StmtKind::Assign { target, value, .. } => {
                    self.validate_expr_trait_objects(target);
                    self.validate_expr_trait_objects(value);
                }
                StmtKind::Return { value } => {
                    if let Some(value) = value {
                        self.validate_expr_trait_objects(value);
                    }
                }
                StmtKind::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    self.validate_expr_trait_objects(condition);
                    self.validate_block_trait_objects(then_branch);
                    if let Some(else_branch) = else_branch {
                        match else_branch {
                            ElseBranch::Block(block) => self.validate_block_trait_objects(block),
                            ElseBranch::If(statement) => {
                                let block = Block {
                                    statements: vec![(**statement).clone()],
                                    span: statement.span,
                                };
                                self.validate_block_trait_objects(&block);
                            }
                        }
                    }
                }
                StmtKind::While { condition, body } => {
                    self.validate_expr_trait_objects(condition);
                    self.validate_block_trait_objects(body);
                }
                StmtKind::For { iterable, body, .. } => {
                    self.validate_expr_trait_objects(iterable);
                    self.validate_block_trait_objects(body);
                }
                StmtKind::Expr(expr) => self.validate_expr_trait_objects(expr),
                StmtKind::Break | StmtKind::Continue => {}
            }
        }
    }

    fn validate_expr_trait_objects(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Array(items) => {
                for item in items {
                    self.validate_expr_trait_objects(item);
                }
            }
            ExprKind::CollectionLiteral { items, collection } => {
                self.validate_trait_object_type(collection);
                for item in items {
                    self.validate_expr_trait_objects(item);
                }
            }
            ExprKind::Call { callee, args } => {
                self.validate_expr_trait_objects(callee);
                for arg in args {
                    self.validate_expr_trait_objects(arg);
                }
            }
            ExprKind::Member { object, .. } => self.validate_expr_trait_objects(object),
            ExprKind::GenericMember { object, args, .. } => {
                self.validate_expr_trait_objects(object);
                for arg in args {
                    self.validate_trait_object_type(arg);
                }
            }
            ExprKind::GenericType { args, .. } => {
                for arg in args {
                    self.validate_trait_object_type(arg);
                }
            }
            ExprKind::StructInit { args, fields, .. } => {
                for arg in args {
                    self.validate_trait_object_type(arg);
                }
                for field in fields {
                    self.validate_expr_trait_objects(&field.value);
                }
            }
            ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
                self.validate_expr_trait_objects(start);
                self.validate_expr_trait_objects(end);
            }
            ExprKind::Cast { value, type_ref } => {
                self.validate_expr_trait_objects(value);
                self.validate_trait_object_type(type_ref);
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.validate_expr_trait_objects(operand)
            }
            ExprKind::Match { value, branches } => {
                self.validate_expr_trait_objects(value);
                for branch in branches {
                    if let Some(guard) = &branch.guard {
                        self.validate_expr_trait_objects(guard);
                    }
                    match &branch.body {
                        MatchBranchBody::Expr(expr) => self.validate_expr_trait_objects(expr),
                        MatchBranchBody::Block(block) => self.validate_block_trait_objects(block),
                    }
                }
            }
            ExprKind::Lambda(function) => self.validate_function_associated_types(function),
            ExprKind::Identifier(_)
            | ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Bool(_)
            | ExprKind::Missing => {}
        }
    }

    fn validate_associated_bindings(&mut self, type_ref: &TypeRef) {
        if type_ref.bindings.is_empty() {
            return;
        }
        let Some(trait_) = self.trait_declarations.get(&type_ref.name) else {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!(
                    "associated-type bindings are only allowed on traits, not `{}`",
                    type_ref.name
                ),
            ));
            return;
        };
        let declared = trait_
            .associated_types
            .iter()
            .map(|associated_type| associated_type.name.as_str())
            .collect::<HashSet<_>>();
        let mut bound = HashSet::new();
        for binding in &type_ref.bindings {
            if !bound.insert(binding.name.as_str()) {
                self.diagnostics.push(Diagnostic::error(
                    binding.span,
                    format!(
                        "duplicate binding for associated type `{}.{}`",
                        trait_.name, binding.name
                    ),
                ));
            } else if !declared.contains(binding.name.as_str()) {
                self.diagnostics.push(Diagnostic::error(
                    binding.span,
                    format!(
                        "trait `{}` does not declare associated type `{}`",
                        trait_.name, binding.name
                    ),
                ));
            }
        }
    }

    fn resolve_associated_projection(
        &self,
        receiver: &TypeRef,
        associated_type: &str,
        associated_args: &[TypeRef],
    ) -> Result<TypeRef, usize> {
        let receiver = self.expanded_type(receiver);
        let mut candidates = Vec::new();
        for impl_ in &self.impl_declarations {
            let Some(trait_) = self.trait_declarations.get(&impl_.trait_ref.name) else {
                continue;
            };
            let Some(declaration) = trait_
                .associated_types
                .iter()
                .find(|declaration| declaration.name == associated_type)
            else {
                continue;
            };
            if declaration.type_params.len() != associated_args.len() {
                continue;
            }
            let Ok(args) = self.solve_type_arguments(
                "associated type projection",
                &impl_.type_params,
                vec![(impl_.type_ref.clone(), receiver.clone())],
            ) else {
                continue;
            };
            let mut substitutions = impl_
                .type_params
                .iter()
                .cloned()
                .zip(args)
                .collect::<HashMap<_, _>>();
            let candidate_receiver =
                self.expanded_type(&substitute_type(&impl_.type_ref, &substitutions));
            if type_name(&candidate_receiver) != type_name(&receiver) {
                continue;
            }
            let definition = impl_
                .associated_types
                .iter()
                .find(|definition| definition.name == associated_type)
                .map(|definition| {
                    (
                        definition.type_params.as_slice(),
                        definition.type_ref.clone(),
                    )
                })
                .or_else(|| {
                    declaration.default.as_ref().map(|default| {
                        (declaration.type_params.as_slice(), default.clone())
                    })
                });
            let Some((type_params, type_ref)) = definition else {
                continue;
            };
            if type_params.len() != associated_args.len() {
                continue;
            }

            let trait_substitutions = trait_
                .type_params
                .iter()
                .cloned()
                .zip(impl_.trait_ref.args.iter().cloned())
                .map(|(name, type_ref)| (name, substitute_type(&type_ref, &substitutions)))
                .collect::<Vec<_>>();
            substitutions.extend(trait_substitutions);
            substitutions.insert("Self".to_string(), receiver.clone());
            substitutions.extend(
                type_params
                    .iter()
                    .cloned()
                    .zip(associated_args.iter().cloned()),
            );
            candidates.push(substitute_type(&type_ref, &substitutions));
        }
        if candidates.len() == 1 {
            Ok(candidates.remove(0))
        } else {
            Err(candidates.len())
        }
    }

    fn apply_associated_type_defaults(&self, impl_: &mut ImplDecl) {
        let Some(trait_) = self.trait_declarations.get(&impl_.trait_ref.name).cloned() else {
            return;
        };
        let substitutions = trait_
            .type_params
            .iter()
            .cloned()
            .zip(impl_.trait_ref.args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for declaration in &trait_.associated_types {
            if impl_
                .associated_types
                .iter()
                .any(|definition| definition.name == declaration.name)
            {
                continue;
            }
            let Some(default) = &declaration.default else {
                continue;
            };
            impl_.associated_types.push(crate::ast::AssociatedTypeDef {
                name: declaration.name.clone(),
                type_params: declaration.type_params.clone(),
                type_param_bounds: declaration
                    .type_param_bounds
                    .iter()
                    .map(|bound| TypeParamBound {
                        param: bound.param.clone(),
                        trait_ref: substitute_type(&bound.trait_ref, &substitutions),
                        span: bound.span,
                    })
                    .collect(),
                type_ref: substitute_type(default, &substitutions),
                span: declaration.span,
            });
        }
    }

    fn record_associated_type_bound_checks(&mut self, impl_: &ImplDecl) {
        let Some(trait_) = self.trait_declarations.get(&impl_.trait_ref.name).cloned() else {
            return;
        };
        let substitutions = trait_
            .type_params
            .iter()
            .cloned()
            .zip(impl_.trait_ref.args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for declaration in &trait_.associated_types {
            if !declaration.type_params.is_empty() {
                continue;
            }
            let Some(definition) = impl_
                .associated_types
                .iter()
                .find(|definition| definition.name == declaration.name)
            else {
                continue;
            };
            for bound in &declaration.bounds {
                let mut trait_ref = substitute_type(bound, &substitutions);
                self.rewrite_type(&mut trait_ref, &HashMap::new());
                self.bound_checks.push(BoundCheck {
                    owner: format!(
                        "associated type `{}.{}`",
                        trait_.name, declaration.name
                    ),
                    type_ref: definition.type_ref.clone(),
                    trait_ref,
                    span: definition.span,
                });
            }
        }
    }
}

fn trait_requires_associated_bindings(trait_: &TraitDecl) -> bool {
    !trait_required_associated_type_names(trait_).is_empty()
}

fn trait_required_associated_type_names(trait_: &TraitDecl) -> HashSet<String> {
    trait_
        .associated_types
        .iter()
                .filter(|associated_type| {
                    associated_type.type_params.is_empty()
                        && trait_.methods.iter().any(|method| {
                method
                    .params
                    .iter()
                    .filter_map(|param| param.type_ref.as_ref())
                    .any(|type_ref| {
                        type_ref_contains_name(
                            type_ref,
                            &format!("Self.{}", associated_type.name),
                        )
                    })
                    || method.return_type.as_ref().is_some_and(|type_ref| {
                        type_ref_contains_name(
                            type_ref,
                            &format!("Self.{}", associated_type.name),
                        )
                    })
            })
        })
        .map(|associated_type| associated_type.name.clone())
        .collect()
}

fn type_ref_contains_name(type_ref: &TypeRef, name: &str) -> bool {
    type_ref.name == name
        || type_ref
            .args
            .iter()
            .any(|arg| type_ref_contains_name(arg, name))
        || type_ref
            .bindings
            .iter()
            .any(|binding| type_ref_contains_name(&binding.type_ref, name))
        || type_ref.function.as_ref().is_some_and(|function| {
            function
                .params
                .iter()
                .any(|param| type_ref_contains_name(&param.type_ref, name))
                || type_ref_contains_name(&function.return_type, name)
        })
}
