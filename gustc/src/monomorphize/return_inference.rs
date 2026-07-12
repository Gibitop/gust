impl Monomorphizer {
    fn infer_specialized_member_returns(&mut self, owner: &str, members: &mut [StructMember]) {
        for _ in 0..members.len() {
            let mut changed = false;
            for member in members.iter() {
                let (function, static_) = match member {
                    StructMember::Method(function) => (function, false),
                    StructMember::StaticMethod(function) => (function, true),
                    StructMember::Field(_) => continue,
                };
                if !function.type_params.is_empty() {
                    continue;
                }
                if let Some(name) = &function.name
                    && let Some(return_type) = &function.return_type
                {
                    self.member_returns.insert(
                        (owner.to_string(), name.clone(), static_),
                        return_type.clone(),
                    );
                }
            }
            for member in members.iter_mut() {
                let (function, static_) = match member {
                    StructMember::Method(function) => (function, false),
                    StructMember::StaticMethod(function) => (function, true),
                    StructMember::Field(_) => continue,
                };
                if !function.type_params.is_empty() {
                    continue;
                }
                if function.return_type.is_some() {
                    continue;
                }
                let Some(return_type) =
                    self.infer_rewritten_function_return(function, owner, !static_)
                else {
                    continue;
                };
                function.return_type = Some(return_type.clone());
                if let Some(name) = &function.name {
                    self.member_returns
                        .insert((owner.to_string(), name.clone(), static_), return_type);
                }
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    fn infer_rewritten_function_return(
        &mut self,
        function: &FunctionDecl,
        self_type: &str,
        has_self: bool,
    ) -> Option<TypeRef> {
        let self_type = TypeRef {
            name: self_type.to_string(),
            args: Vec::new(),
            function: None,
            span: function.span,
        };
        let mut scope = function
            .params
            .iter()
            .filter_map(|param| {
                param
                    .type_ref
                    .as_ref()
                    .map(|type_ref| (param.name.clone(), type_ref.clone()))
            })
            .collect::<HashMap<_, _>>();
        scope.insert("Self".to_string(), self_type.clone());
        if has_self {
            scope.insert("self".to_string(), self_type);
        }
        self.scopes.push(scope);
        let return_types = match &function.body {
            FunctionBody::Expr(expr) => self.infer_expr_type(expr).into_iter().collect(),
            FunctionBody::Block(block) => self.infer_block_return_types(block),
        };
        self.scopes.pop();
        consistent_type(&return_types)
    }

    fn infer_block_return_types(&mut self, block: &Block) -> Vec<TypeRef> {
        self.scopes.push(HashMap::new());
        let mut return_types = Vec::new();
        for statement in &block.statements {
            match &statement.kind {
                StmtKind::Let {
                    name,
                    type_annotation,
                    value,
                    ..
                } => {
                    let type_ref = type_annotation
                        .clone()
                        .or_else(|| value.as_ref().and_then(|value| self.infer_expr_type(value)));
                    if let Some(type_ref) = type_ref
                        && let Some(scope) = self.scopes.last_mut()
                    {
                        scope.insert(name.clone(), type_ref);
                    }
                }
                StmtKind::Return { value: Some(value) } => {
                    if let Some(type_ref) = self.infer_expr_type(value) {
                        return_types.push(type_ref);
                    }
                }
                StmtKind::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    return_types.extend(self.infer_block_return_types(then_branch));
                    if let Some(else_branch) = else_branch {
                        match else_branch {
                            ElseBranch::Block(block) => {
                                return_types.extend(self.infer_block_return_types(block));
                            }
                            ElseBranch::If(statement) => {
                                let block = Block {
                                    statements: vec![(**statement).clone()],
                                    span: statement.span,
                                };
                                return_types.extend(self.infer_block_return_types(&block));
                            }
                        }
                    }
                }
                StmtKind::For { body, .. } => {
                    return_types.extend(self.infer_block_return_types(body));
                }
                StmtKind::While { body, .. } => {
                    return_types.extend(self.infer_block_return_types(body));
                }
                StmtKind::Assign { .. }
                | StmtKind::Return { value: None }
                | StmtKind::Break
                | StmtKind::Continue
                | StmtKind::Expr(_) => {}
            }
        }
        self.scopes.pop();
        return_types
    }

}
