fn captured_let_names(function: &FunctionDecl) -> HashSet<String> {
    let mut captured = HashSet::new();
    let mut available = HashSet::new();
    match &function.body {
        FunctionBody::Block(block) => collect_block_captures(block, &mut available, &mut captured),
        FunctionBody::Expr(expr) => collect_expr_captures(expr, &available, &mut captured),
    }
    captured
}

fn collect_block_captures(
    block: &Block,
    available: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { name, value, .. } => {
                if let Some(value) = value {
                    collect_expr_captures(value, available, captured);
                }
                available.insert(name.clone());
            }
            StmtKind::Assign { target, value, .. } => {
                collect_expr_captures(target, available, captured);
                collect_expr_captures(value, available, captured);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value.as_ref() {
                    collect_expr_captures(value, available, captured);
                }
            }
            StmtKind::Expr(value) => collect_expr_captures(value, available, captured),
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_expr_captures(condition, available, captured);
                let mut branch_available = available.clone();
                collect_block_captures(then_branch, &mut branch_available, captured);
                if let Some(else_branch) = else_branch {
                    let mut branch_available = available.clone();
                    match else_branch {
                        ElseBranch::Block(block) => {
                            collect_block_captures(block, &mut branch_available, captured)
                        }
                        ElseBranch::If(statement) => {
                            collect_statement_captures(statement, &mut branch_available, captured)
                        }
                    }
                }
            }
            StmtKind::While { condition, body } => {
                collect_expr_captures(condition, available, captured);
                let mut body_available = available.clone();
                collect_block_captures(body, &mut body_available, captured);
            }
            StmtKind::For {
                iterable,
                body,
                name,
            } => {
                collect_expr_captures(iterable, available, captured);
                let mut body_available = available.clone();
                body_available.insert(name.clone());
                collect_block_captures(body, &mut body_available, captured);
            }
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

fn collect_statement_captures(
    statement: &Stmt,
    available: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    let block = Block {
        statements: vec![statement.clone()],
        span: statement.span,
    };
    collect_block_captures(&block, available, captured);
}

fn collect_expr_captures(expr: &Expr, available: &HashSet<String>, captured: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Lambda(function) => {
            let mut lambda_locals = HashSet::new();
            for param in &function.params {
                lambda_locals.insert(param.name.clone());
            }
            collect_lambda_body_captures(function, available, &mut lambda_locals, captured);
        }
        ExprKind::Call { callee, args } => {
            collect_expr_captures(callee, available, captured);
            for arg in args {
                collect_expr_captures(arg, available, captured);
            }
        }
        ExprKind::Member { object, .. }
        | ExprKind::GenericMember { object, .. }
        | ExprKind::Unary {
            operand: object, ..
        }
        | ExprKind::Cast { value: object, .. }
        | ExprKind::PostfixIncrement(object) => collect_expr_captures(object, available, captured),
        ExprKind::Binary { left, right, .. } => {
            collect_expr_captures(left, available, captured);
            collect_expr_captures(right, available, captured);
        }
        ExprKind::Array(items) => {
            for item in items {
                collect_expr_captures(item, available, captured);
            }
        }
        ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_expr_captures(item, available, captured);
            }
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_expr_captures(&field.value, available, captured);
            }
        }
        ExprKind::Range { start, end, .. } => {
            collect_expr_captures(start, available, captured);
            collect_expr_captures(end, available, captured);
        }
        ExprKind::Match { value, branches } => {
            collect_expr_captures(value, available, captured);
            for branch in branches {
                let mut branch_available = available.clone();
                collect_pattern_bindings(&branch.pattern, &mut branch_available);
                if let Some(guard) = &branch.guard {
                    collect_expr_captures(guard, &branch_available, captured);
                }
                match &branch.body {
                    MatchBranchBody::Expr(expr) => {
                        collect_expr_captures(expr, &branch_available, captured)
                    }
                    MatchBranchBody::Block(block) => collect_block_captures(
                        block,
                        &mut branch_available,
                        captured,
                    ),
                }
            }
        }
        ExprKind::Identifier(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Missing => {}
    }
}

fn collect_lambda_body_captures(
    function: &FunctionDecl,
    available: &HashSet<String>,
    lambda_locals: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    match &function.body {
        FunctionBody::Expr(expr) => {
            collect_lambda_expr_captures(expr, available, lambda_locals, captured)
        }
        FunctionBody::Block(block) => {
            for statement in &block.statements {
                match &statement.kind {
                    StmtKind::Let { name, value, .. } => {
                        if let Some(value) = value {
                            collect_lambda_expr_captures(value, available, lambda_locals, captured);
                        }
                        lambda_locals.insert(name.clone());
                    }
                    StmtKind::Assign { target, value, .. } => {
                        collect_lambda_expr_captures(target, available, lambda_locals, captured);
                        collect_lambda_expr_captures(value, available, lambda_locals, captured);
                    }
                    StmtKind::Return { value } => {
                        if let Some(value) = value.as_ref() {
                            collect_lambda_expr_captures(value, available, lambda_locals, captured);
                        }
                    }
                    StmtKind::Expr(value) => {
                        collect_lambda_expr_captures(value, available, lambda_locals, captured)
                    }
                    StmtKind::If {
                        condition,
                        then_branch,
                        else_branch,
                    } => {
                        collect_lambda_expr_captures(condition, available, lambda_locals, captured);
                        let mut branch_locals = lambda_locals.clone();
                        collect_lambda_block_captures(
                            then_branch,
                            available,
                            &mut branch_locals,
                            captured,
                        );
                        if let Some(else_branch) = else_branch {
                            let mut branch_locals = lambda_locals.clone();
                            match else_branch {
                                ElseBranch::Block(block) => collect_lambda_block_captures(
                                    block,
                                    available,
                                    &mut branch_locals,
                                    captured,
                                ),
                                ElseBranch::If(statement) => {
                                    let block = Block {
                                        statements: vec![(**statement).clone()],
                                        span: statement.span,
                                    };
                                    collect_lambda_block_captures(
                                        &block,
                                        available,
                                        &mut branch_locals,
                                        captured,
                                    );
                                }
                            }
                        }
                    }
                    StmtKind::While { condition, body } => {
                        collect_lambda_expr_captures(condition, available, lambda_locals, captured);
                        let mut body_locals = lambda_locals.clone();
                        collect_lambda_block_captures(body, available, &mut body_locals, captured);
                    }
                    StmtKind::For {
                        iterable,
                        body,
                        name,
                    } => {
                        collect_lambda_expr_captures(iterable, available, lambda_locals, captured);
                        let mut body_locals = lambda_locals.clone();
                        body_locals.insert(name.clone());
                        collect_lambda_block_captures(body, available, &mut body_locals, captured);
                    }
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
        }
    }
}

fn collect_lambda_block_captures(
    block: &Block,
    available: &HashSet<String>,
    lambda_locals: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    let function = FunctionDecl {
        name: None,
        type_params: Vec::new(),
        type_param_bounds: Vec::new(),
        params: Vec::new(),
        return_type: None,
        body: FunctionBody::Block(block.clone()),
        span: block.span,
    };
    collect_lambda_body_captures(&function, available, lambda_locals, captured);
}

fn collect_lambda_expr_captures(
    expr: &Expr,
    available: &HashSet<String>,
    lambda_locals: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            if available.contains(name) && !lambda_locals.contains(name) {
                captured.insert(name.clone());
            }
        }
        ExprKind::Lambda(function) => {
            let mut nested_locals = lambda_locals.clone();
            for param in &function.params {
                nested_locals.insert(param.name.clone());
            }
            collect_lambda_body_captures(function, available, &mut nested_locals, captured);
        }
        ExprKind::Call { callee, args } => {
            collect_lambda_expr_captures(callee, available, lambda_locals, captured);
            for arg in args {
                collect_lambda_expr_captures(arg, available, lambda_locals, captured);
            }
        }
        ExprKind::Member { object, .. }
        | ExprKind::GenericMember { object, .. }
        | ExprKind::Unary {
            operand: object, ..
        }
        | ExprKind::Cast { value: object, .. }
        | ExprKind::PostfixIncrement(object) => {
            collect_lambda_expr_captures(object, available, lambda_locals, captured)
        }
        ExprKind::Binary { left, right, .. } => {
            collect_lambda_expr_captures(left, available, lambda_locals, captured);
            collect_lambda_expr_captures(right, available, lambda_locals, captured);
        }
        ExprKind::Array(items) => {
            for item in items {
                collect_lambda_expr_captures(item, available, lambda_locals, captured);
            }
        }
        ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_lambda_expr_captures(item, available, lambda_locals, captured);
            }
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_lambda_expr_captures(&field.value, available, lambda_locals, captured);
            }
        }
        ExprKind::Range { start, end, .. } => {
            collect_lambda_expr_captures(start, available, lambda_locals, captured);
            collect_lambda_expr_captures(end, available, lambda_locals, captured);
        }
        ExprKind::Match { value, branches } => {
            collect_lambda_expr_captures(value, available, lambda_locals, captured);
            for branch in branches {
                let mut branch_locals = lambda_locals.clone();
                collect_pattern_bindings(&branch.pattern, &mut branch_locals);
                if let Some(guard) = &branch.guard {
                    collect_lambda_expr_captures(guard, available, &mut branch_locals, captured);
                }
                match &branch.body {
                    MatchBranchBody::Expr(expr) => {
                        collect_lambda_expr_captures(expr, available, &mut branch_locals, captured)
                    }
                    MatchBranchBody::Block(block) => collect_lambda_block_captures(
                        block,
                        available,
                        &mut branch_locals,
                        captured,
                    ),
                }
            }
        }
        ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Missing => {}
    }
}

fn collect_pattern_bindings(pattern: &Pattern, bindings: &mut HashSet<String>) {
    match pattern {
        Pattern::Or { alternatives, .. } => {
            for alternative in alternatives {
                collect_pattern_bindings(alternative, bindings);
            }
        }
        Pattern::Variant { payload, .. } => {
            if let Some(payload) = payload {
                collect_pattern_bindings(payload, bindings);
            }
        }
        Pattern::Struct { fields, .. } => {
            for field in fields {
                collect_pattern_bindings(&field.pattern, bindings);
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
