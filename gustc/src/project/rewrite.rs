struct ModuleRewriter<'names, 'diagnostics> {
    local_names: &'names HashMap<String, String>,
    visible_names: &'names HashMap<String, String>,
    local_extensions: &'names HashMap<String, String>,
    visible_extensions: &'names HashMap<String, String>,
    visible_namespaces: &'names HashMap<String, usize>,
    exports: &'names [HashMap<String, Export>],
    diagnostics: &'diagnostics mut Vec<Diagnostic>,
    scopes: Vec<HashSet<String>>,
    entry: bool,
}

impl<'names, 'diagnostics> ModuleRewriter<'names, 'diagnostics> {
    // Module rewriting is initialized from the linker-owned name maps and diagnostics sink; the
    // constructor keeps those borrowed inputs visible instead of hiding them in an intermediate bag.
    #[allow(clippy::too_many_arguments)]
    fn new(
        local_names: &'names HashMap<String, String>,
        visible_names: &'names HashMap<String, String>,
        local_extensions: &'names HashMap<String, String>,
        visible_extensions: &'names HashMap<String, String>,
        visible_namespaces: &'names HashMap<String, usize>,
        exports: &'names [HashMap<String, Export>],
        diagnostics: &'diagnostics mut Vec<Diagnostic>,
        entry: bool,
    ) -> Self {
        Self {
            local_names,
            visible_names,
            local_extensions,
            visible_extensions,
            visible_namespaces,
            exports,
            diagnostics,
            scopes: Vec::new(),
            entry,
        }
    }

    fn rewrite_item(&mut self, item: &mut Item) {
        match item {
            Item::Enum(item) => {
                self.rewrite_declared_name(&mut item.name);
                self.scopes.push(item.type_params.iter().cloned().collect());
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref);
                }
                for variant in &mut item.variants {
                    if let Some(type_ref) = &mut variant.payload {
                        self.rewrite_type(type_ref);
                    }
                }
                for member in &mut item.members {
                    match member {
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            self.rewrite_function(function);
                        }
                        StructMember::Field(field) => self.rewrite_type(&mut field.type_ref),
                    }
                }
                self.scopes.pop();
            }
            Item::Struct(item) => {
                self.rewrite_declared_name(&mut item.name);
                self.scopes.push(item.type_params.iter().cloned().collect());
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref);
                }
                for member in &mut item.members {
                    match member {
                        StructMember::Field(field) => self.rewrite_type(&mut field.type_ref),
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            self.rewrite_function(function);
                        }
                    }
                }
                self.scopes.pop();
            }
            Item::Trait(item) => {
                self.rewrite_declared_name(&mut item.name);
                self.scopes.push(item.type_params.iter().cloned().collect());
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref);
                }
                for method in &mut item.methods {
                    for param in &mut method.params {
                        if let Some(type_ref) = &mut param.type_ref {
                            self.rewrite_type(type_ref);
                        }
                    }
                    if let Some(return_type) = &mut method.return_type {
                        self.rewrite_type(return_type);
                    }
                }
                self.scopes.pop();
            }
            Item::Impl(item) => {
                self.scopes.push(item.type_params.iter().cloned().collect());
                for bound in &mut item.type_param_bounds {
                    self.rewrite_type(&mut bound.trait_ref);
                }
                self.rewrite_type(&mut item.trait_ref);
                self.rewrite_type(&mut item.type_ref);
                for member in &mut item.methods {
                    self.rewrite_function(&mut member.function);
                }
                self.scopes.pop();
            }
            Item::Function(function) => {
                if let Some(name) = &mut function.name
                    && !(self.entry && name == "main")
                {
                    self.rewrite_declared_name(name);
                }
                self.rewrite_function(function);
            }
            Item::Extension(extension) => {
                self.rewrite_type(&mut extension.type_ref);
                if let Some(name) = &mut extension.function.name
                    && let Some(internal_name) = self.local_extensions.get(name)
                {
                    *name = internal_name.clone();
                }
                self.rewrite_function(&mut extension.function);
            }
            Item::Import(_) => {}
        }
    }

    fn rewrite_declared_name(&self, name: &mut String) {
        if let Some(internal_name) = self.local_names.get(name) {
            *name = internal_name.clone();
        }
    }

    fn rewrite_function(&mut self, function: &mut FunctionDecl) {
        self.scopes
            .push(function.type_params.iter().cloned().collect());
        for bound in &mut function.type_param_bounds {
            self.rewrite_type(&mut bound.trait_ref);
        }
        for param in &mut function.params {
            if let Some(type_ref) = &mut param.type_ref {
                self.rewrite_type(type_ref);
            }
        }
        if let Some(return_type) = &mut function.return_type {
            self.rewrite_type(return_type);
        }

        self.scopes.push(
            function
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect(),
        );
        match &mut function.body {
            FunctionBody::Block(block) => self.rewrite_block(block),
            FunctionBody::Expr(expr) => self.rewrite_expr(expr),
        }
        self.scopes.pop();
        self.scopes.pop();
    }

    fn rewrite_block(&mut self, block: &mut Block) {
        self.scopes.push(HashSet::new());
        for statement in &mut block.statements {
            self.rewrite_statement(statement);
        }
        self.scopes.pop();
    }

    fn rewrite_statement(&mut self, statement: &mut Stmt) {
        match &mut statement.kind {
            StmtKind::Let {
                name,
                type_annotation,
                value,
                ..
            } => {
                if let Some(type_ref) = type_annotation {
                    self.rewrite_type(type_ref);
                }
                if let Some(value) = value {
                    self.rewrite_expr(value);
                }
                self.define_local(name);
            }
            StmtKind::Assign { target, value, .. } => {
                self.rewrite_expr(target);
                self.rewrite_expr(value);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    self.rewrite_expr(value);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.rewrite_expr(condition);
                self.rewrite_block(then_branch);
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.rewrite_block(block),
                        ElseBranch::If(statement) => self.rewrite_statement(statement),
                    }
                }
            }
            StmtKind::While { condition, body } => {
                self.rewrite_expr(condition);
                self.rewrite_block(body);
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                self.rewrite_expr(iterable);
                self.scopes.push(HashSet::from([name.clone()]));
                for statement in &mut body.statements {
                    self.rewrite_statement(statement);
                }
                self.scopes.pop();
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) => self.rewrite_expr(expr),
        }
    }

    fn rewrite_expr(&mut self, expr: &mut Expr) {
        if let ExprKind::Member { object, name } = &expr.kind
            && let ExprKind::Identifier(namespace) = &object.kind
            && let Some(internal_name) = self.resolve_namespace_member(namespace, name, expr.span)
        {
            expr.kind = ExprKind::Identifier(internal_name);
            return;
        }

        match &mut expr.kind {
            ExprKind::Identifier(name) => {
                if !self.is_local(name)
                    && let Some(internal_name) = self.visible_names.get(name)
                {
                    *name = internal_name.clone();
                }
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.rewrite_expr(item);
                }
            }
            ExprKind::CollectionLiteral { items, collection } => {
                self.rewrite_type(collection);
                for item in items {
                    self.rewrite_expr(item);
                }
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Member { name, .. } = &mut callee.kind
                    && let Some(internal_name) = self.visible_extensions.get(name)
                {
                    *name = internal_name.clone();
                }
                self.rewrite_expr(callee);
                for arg in args {
                    self.rewrite_expr(arg);
                }
            }
            ExprKind::Member { object, .. } => self.rewrite_expr(object),
            ExprKind::GenericMember { object, args, .. } => {
                self.rewrite_expr(object);
                for arg in args {
                    self.rewrite_type(arg);
                }
            }
            ExprKind::GenericType { name, args } => {
                if let Some(internal_name) = self.resolve_qualified_name(name, expr.span) {
                    *name = internal_name;
                } else if let Some(internal_name) = self.visible_names.get(name) {
                    *name = internal_name.clone();
                }
                for arg in args {
                    self.rewrite_type(arg);
                }
            }
            ExprKind::StructInit { name, args, fields } => {
                if let Some(internal_name) = self.resolve_qualified_name(name, expr.span) {
                    *name = internal_name;
                } else if let Some(internal_name) = self.visible_names.get(name) {
                    *name = internal_name.clone();
                }
                for arg in args {
                    self.rewrite_type(arg);
                }
                for field in fields {
                    self.rewrite_expr(&mut field.value);
                }
            }
            ExprKind::Range { start, end, .. } => {
                self.rewrite_expr(start);
                self.rewrite_expr(end);
            }
            ExprKind::Binary { left, right, .. } => {
                self.rewrite_expr(left);
                self.rewrite_expr(right);
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.rewrite_expr(operand);
            }
            ExprKind::Match { value, branches } => {
                self.rewrite_expr(value);
                for branch in branches {
                    self.rewrite_match_branch(branch);
                }
            }
            ExprKind::Lambda(function) => self.rewrite_function(function),
            ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Bool(_)
            | ExprKind::Missing => {}
        }
    }

    fn rewrite_match_branch(&mut self, branch: &mut MatchBranch) {
        let mut bindings = HashSet::new();
        self.rewrite_pattern(&mut branch.pattern, branch.span, &mut bindings);

        self.scopes.push(bindings);
        match &mut branch.body {
            MatchBranchBody::Expr(expr) => self.rewrite_expr(expr),
            MatchBranchBody::Block(block) => self.rewrite_block(block),
        }
        self.scopes.pop();
    }

    fn rewrite_pattern(
        &mut self,
        pattern: &mut Pattern,
        branch_span: Span,
        bindings: &mut HashSet<String>,
    ) {
        match pattern {
            Pattern::Variant {
                enum_name, payload, ..
            } => {
                if let Some(internal_name) = self.resolve_qualified_name(enum_name, branch_span) {
                    *enum_name = internal_name;
                } else if let Some(internal_name) = self.visible_names.get(enum_name) {
                    *enum_name = internal_name.clone();
                }
                if let Some(payload) = payload {
                    self.rewrite_pattern(payload, branch_span, bindings);
                }
            }
            Pattern::Struct { name, fields, .. } => {
                if let Some(internal_name) = self.resolve_qualified_name(name, branch_span) {
                    *name = internal_name;
                } else if let Some(internal_name) = self.visible_names.get(name) {
                    *name = internal_name.clone();
                }
                for field in fields {
                    self.rewrite_pattern(&mut field.pattern, branch_span, bindings);
                }
            }
            Pattern::Binding { name, .. } if name != "_" => {
                bindings.insert(name.clone());
            }
            Pattern::Binding { .. }
            | Pattern::String { .. }
            | Pattern::Bool { .. }
            | Pattern::Number { .. }
            | Pattern::Range { .. }
            | Pattern::Wildcard { .. } => {}
        }
    }

    fn rewrite_type(&self, type_ref: &mut TypeRef) {
        if let Some(function) = &mut type_ref.function {
            for param in &mut function.params {
                self.rewrite_type(&mut param.type_ref);
            }
            self.rewrite_type(&mut function.return_type);
            return;
        }

        if self.is_local(&type_ref.name) {
            return;
        }
        if let Some(internal_name) =
            resolve_qualified_name(self.visible_namespaces, self.exports, &type_ref.name)
        {
            type_ref.name = internal_name;
        } else if let Some(internal_name) = self.visible_names.get(&type_ref.name) {
            type_ref.name = internal_name.clone();
        }
        for arg in &mut type_ref.args {
            self.rewrite_type(arg);
        }
    }

    fn define_local(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn resolve_qualified_name(&mut self, name: &str, span: Span) -> Option<String> {
        let (namespace, member) = name.split_once('.')?;
        self.resolve_namespace_member(namespace, member, span)
    }

    fn resolve_namespace_member(
        &mut self,
        namespace: &str,
        member: &str,
        span: Span,
    ) -> Option<String> {
        let target = self.visible_namespaces.get(namespace)?;
        let Some(export) = self.exports[*target].get(member) else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("module namespace `{namespace}` does not export `{member}`"),
            ));
            return None;
        };
        if export.extension {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "extension function `{member}` must be imported by name to participate in method lookup"
                ),
            ));
            return None;
        }
        Some(export.internal_name.clone())
    }
}

fn resolve_qualified_name(
    visible_namespaces: &HashMap<String, usize>,
    exports: &[HashMap<String, Export>],
    name: &str,
) -> Option<String> {
    let (namespace, member) = name.split_once('.')?;
    let target = visible_namespaces.get(namespace)?;
    let export = exports[*target].get(member)?;
    (!export.extension).then(|| export.internal_name.clone())
}
