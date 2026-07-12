impl Monomorphizer {
    fn rewrite_item(&mut self, item: &mut Item, substitutions: &HashMap<String, TypeRef>) {
        match item {
            Item::Import(_) => {}
            Item::Enum(item) => {
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref, substitutions);
                }
                self.self_types.push(TypeRef {
                    name: item.name.clone(),
                    args: Vec::new(),
                    function: None,
                    span: item.span,
                });
                for variant in &mut item.variants {
                    if let Some(payload) = &mut variant.payload {
                        self.rewrite_type(payload, substitutions);
                    }
                }
                for member in &mut item.members {
                    match member {
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            if function.type_params.is_empty() {
                                self.rewrite_function(function, substitutions);
                            }
                        }
                        StructMember::Field(_) => {}
                    }
                }
                self.self_types.pop();
            }
            Item::Struct(item) => {
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref, substitutions);
                }
                self.self_types.push(TypeRef {
                    name: item.name.clone(),
                    args: Vec::new(),
                    function: None,
                    span: item.span,
                });
                for member in &mut item.members {
                    match member {
                        StructMember::Field(field) => {
                            self.rewrite_type(&mut field.type_ref, substitutions);
                        }
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            if function.type_params.is_empty() {
                                self.rewrite_function(function, substitutions);
                            }
                        }
                    }
                }
                self.self_types.pop();
            }
            Item::Trait(item) => {
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref, substitutions);
                }
                for method in &mut item.methods {
                    for param in &mut method.params {
                        if let Some(type_ref) = &mut param.type_ref {
                            self.rewrite_type(type_ref, substitutions);
                        }
                    }
                    if let Some(return_type) = &mut method.return_type {
                        self.rewrite_type(return_type, substitutions);
                    }
                }
            }
            Item::Impl(item) => {
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref, substitutions);
                }
                self.rewrite_type(&mut item.trait_ref, substitutions);
                self.rewrite_type(&mut item.type_ref, substitutions);
                self.self_types.push(item.type_ref.clone());
                for member in &mut item.methods {
                    self.rewrite_function(&mut member.function, substitutions);
                }
                self.self_types.pop();
            }
            Item::Extension(item) => {
                self.rewrite_type(&mut item.type_ref, substitutions);
                self.rewrite_function(&mut item.function, substitutions);
            }
            Item::Function(function) => {
                self.rewrite_function(function, substitutions);
                if let (Some(name), Some(return_type)) = (&function.name, &function.return_type) {
                    self.function_returns
                        .insert(name.clone(), return_type.clone());
                }
                if let Some(name) = &function.name {
                    self.function_params.insert(
                        name.clone(),
                        function
                            .params
                            .iter()
                            .map(|param| param.type_ref.clone())
                            .collect(),
                    );
                }
            }
        }
    }

    fn rewrite_function(
        &mut self,
        function: &mut FunctionDecl,
        substitutions: &HashMap<String, TypeRef>,
    ) {
        for bound in &mut function.type_param_bounds {
            self.rewrite_type(&mut bound.trait_ref, substitutions);
        }
        for param in &mut function.params {
            if let Some(type_ref) = &mut param.type_ref {
                self.rewrite_type(type_ref, substitutions);
            }
        }
        if let Some(return_type) = &mut function.return_type {
            self.rewrite_type(return_type, substitutions);
        }
        let mut function_scope = function
            .params
            .iter()
            .filter_map(|param| {
                param
                    .type_ref
                    .as_ref()
                    .map(|type_ref| (param.name.clone(), type_ref.clone()))
            })
            .collect::<HashMap<_, _>>();
        if let Some(self_type) = self.self_types.last() {
            function_scope.insert("Self".to_string(), self_type.clone());
            function_scope.insert("self".to_string(), self_type.clone());
        }
        self.scopes.push(function_scope);
        let had_explicit_return = function.return_type.is_some();
        if let Some(return_type) = &function.return_type {
            self.return_types.push(return_type.clone());
        }
        let infer_return = !had_explicit_return
            && self
                .self_types
                .last()
                .is_some_and(|type_ref| self.specializations.contains_key(&type_ref.name));
        self.inferred_returns
            .push(infer_return.then(Vec::<TypeRef>::new));
        match &mut function.body {
            FunctionBody::Block(block) => self.rewrite_block(block, substitutions),
            FunctionBody::Expr(expr) => {
                if let Some(return_type) = self.return_types.last().cloned() {
                    self.apply_expr_context(expr, &return_type);
                }
                self.rewrite_expr(expr, substitutions);
                if infer_return
                    && let Some(type_ref) = self.infer_expr_type(expr)
                    && let Some(Some(return_types)) = self.inferred_returns.last_mut()
                {
                    return_types.push(type_ref);
                }
            }
        }
        if let Some(Some(return_types)) = self.inferred_returns.pop()
            && let Some(return_type) = consistent_type(&return_types)
        {
            function.return_type = Some(return_type);
        }
        if had_explicit_return {
            self.return_types.pop();
        }
        self.scopes.pop();
    }

    fn rewrite_block(&mut self, block: &mut Block, substitutions: &HashMap<String, TypeRef>) {
        self.scopes.push(HashMap::new());
        for statement in &mut block.statements {
            self.rewrite_statement(statement, substitutions);
        }
        self.scopes.pop();
    }

    fn rewrite_statement(
        &mut self,
        statement: &mut Stmt,
        substitutions: &HashMap<String, TypeRef>,
    ) {
        match &mut statement.kind {
            StmtKind::Let {
                name,
                type_annotation,
                value,
                ..
            } => {
                if let Some(type_ref) = type_annotation {
                    self.rewrite_type(type_ref, substitutions);
                    if let Some(value) = value {
                        self.apply_expr_context(value, type_ref);
                    }
                }
                if let Some(value) = value {
                    self.rewrite_expr(value, substitutions);
                }
                let inferred_type = type_annotation
                    .clone()
                    .or_else(|| value.as_ref().and_then(|value| self.infer_expr_type(value)));
                if let Some(type_ref) = inferred_type
                    && let Some(scope) = self.scopes.last_mut()
                {
                    scope.insert(name.clone(), type_ref);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                if let Some(expected) = self.infer_expr_type(target) {
                    self.apply_expr_context(value, &expected);
                }
                self.rewrite_expr(target, substitutions);
                self.rewrite_expr(value, substitutions);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    if let Some(return_type) = self.return_types.last().cloned() {
                        self.apply_expr_context(value, &return_type);
                    }
                    self.rewrite_expr(value, substitutions);
                    if let Some(type_ref) = self.infer_expr_type(value)
                        && let Some(Some(return_types)) = self.inferred_returns.last_mut()
                    {
                        return_types.push(type_ref);
                    }
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.rewrite_expr(condition, substitutions);
                self.rewrite_block(then_branch, substitutions);
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.rewrite_block(block, substitutions),
                        ElseBranch::If(statement) => {
                            self.rewrite_statement(statement, substitutions);
                        }
                    }
                }
            }
            StmtKind::While { condition, body } => {
                self.rewrite_expr(condition, substitutions);
                self.rewrite_block(body, substitutions);
            }
            StmtKind::For { iterable, body, .. } => {
                self.rewrite_expr(iterable, substitutions);
                self.rewrite_block(body, substitutions);
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) => self.rewrite_expr(expr, substitutions),
        }
    }

    fn rewrite_type(&mut self, type_ref: &mut TypeRef, substitutions: &HashMap<String, TypeRef>) {
        if let Some(function) = &mut type_ref.function {
            for param in &mut function.params {
                self.rewrite_type(&mut param.type_ref, substitutions);
            }
            self.rewrite_type(&mut function.return_type, substitutions);
            return;
        }

        if type_ref.args.is_empty()
            && let Some(substitution) = substitutions.get(&type_ref.name)
        {
            let span = type_ref.span;
            *type_ref = substitution.clone();
            type_ref.span = span;
            return;
        }

        for arg in &mut type_ref.args {
            self.rewrite_type(arg, substitutions);
        }

        if self.struct_templates.contains_key(&type_ref.name) {
            let name = type_ref.name.clone();
            self.specialize_struct(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        } else if self.enum_templates.contains_key(&type_ref.name) {
            let name = type_ref.name.clone();
            self.specialize_enum(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        } else if self.trait_templates.contains_key(&type_ref.name) {
            let name = type_ref.name.clone();
            self.specialize_trait(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        } else if self.concrete_structs.contains(&type_ref.name) && !type_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("struct `{}` does not accept type arguments", type_ref.name),
            ));
        } else if self.concrete_enums.contains_key(&type_ref.name) && !type_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("enum `{}` does not accept type arguments", type_ref.name),
            ));
        } else if self.concrete_traits.contains(&type_ref.name) && !type_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("trait `{}` does not accept type arguments", type_ref.name),
            ));
        }
    }

}
