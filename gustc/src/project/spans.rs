fn shift_diagnostic(mut diagnostic: Diagnostic, offset: usize) -> Diagnostic {
    shift_span(&mut diagnostic.span, offset);
    diagnostic
}

fn shift_program(program: &mut Program, offset: usize) {
    for item in &mut program.items {
        match item {
            Item::Import(item) => {
                shift_span(&mut item.span, offset);
                for name in &mut item.names {
                    shift_span(&mut name.span, offset);
                }
                if let Some(namespace) = &mut item.namespace {
                    shift_span(&mut namespace.span, offset);
                }
            }
            Item::Enum(item) => {
                shift_span(&mut item.span, offset);
                for variant in &mut item.variants {
                    shift_span(&mut variant.span, offset);
                    if let Some(type_ref) = &mut variant.payload {
                        shift_type(type_ref, offset);
                    }
                }
                for member in &mut item.members {
                    match member {
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            shift_function(function, offset);
                        }
                        StructMember::Field(field) => {
                            shift_span(&mut field.span, offset);
                            shift_type(&mut field.type_ref, offset);
                        }
                    }
                }
            }
            Item::Struct(item) => {
                shift_span(&mut item.span, offset);
                for member in &mut item.members {
                    match member {
                        StructMember::Field(field) => {
                            shift_span(&mut field.span, offset);
                            shift_type(&mut field.type_ref, offset);
                        }
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            shift_function(function, offset);
                        }
                    }
                }
            }
            Item::Trait(item) => {
                shift_span(&mut item.span, offset);
                for method in &mut item.methods {
                    shift_span(&mut method.span, offset);
                    for param in &mut method.params {
                        shift_param(param, offset);
                    }
                    if let Some(return_type) = &mut method.return_type {
                        shift_type(return_type, offset);
                    }
                }
            }
            Item::Impl(item) => {
                shift_span(&mut item.span, offset);
                shift_type(&mut item.trait_ref, offset);
                shift_type(&mut item.type_ref, offset);
                for member in &mut item.methods {
                    shift_span(&mut member.span, offset);
                    shift_function(&mut member.function, offset);
                }
            }
            Item::Extension(item) => {
                shift_span(&mut item.span, offset);
                shift_type(&mut item.type_ref, offset);
                shift_function(&mut item.function, offset);
            }
            Item::Function(function) => shift_function(function, offset),
        }
    }
}

fn shift_function(function: &mut FunctionDecl, offset: usize) {
    shift_span(&mut function.span, offset);
    for param in &mut function.params {
        shift_param(param, offset);
    }
    if let Some(return_type) = &mut function.return_type {
        shift_type(return_type, offset);
    }
    match &mut function.body {
        FunctionBody::Block(block) => shift_block(block, offset),
        FunctionBody::Expr(expr) => shift_expr(expr, offset),
    }
}

fn shift_param(param: &mut Param, offset: usize) {
    shift_span(&mut param.span, offset);
    if let Some(type_ref) = &mut param.type_ref {
        shift_type(type_ref, offset);
    }
}

fn shift_type(type_ref: &mut TypeRef, offset: usize) {
    shift_span(&mut type_ref.span, offset);
    if let Some(function) = &mut type_ref.function {
        for param in &mut function.params {
            shift_type(&mut param.type_ref, offset);
        }
        shift_type(&mut function.return_type, offset);
        return;
    }

    for arg in &mut type_ref.args {
        shift_type(arg, offset);
    }
}

fn shift_block(block: &mut Block, offset: usize) {
    shift_span(&mut block.span, offset);
    for statement in &mut block.statements {
        shift_statement(statement, offset);
    }
}

fn shift_statement(statement: &mut Stmt, offset: usize) {
    shift_span(&mut statement.span, offset);
    match &mut statement.kind {
        StmtKind::Let {
            type_annotation,
            value,
            ..
        } => {
            if let Some(type_ref) = type_annotation {
                shift_type(type_ref, offset);
            }
            if let Some(value) = value {
                shift_expr(value, offset);
            }
        }
        StmtKind::Assign { target, value, .. } => {
            shift_expr(target, offset);
            shift_expr(value, offset);
        }
        StmtKind::Return { value } => {
            if let Some(value) = value {
                shift_expr(value, offset);
            }
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            shift_expr(condition, offset);
            shift_block(then_branch, offset);
            if let Some(else_branch) = else_branch {
                match else_branch {
                    ElseBranch::Block(block) => shift_block(block, offset),
                    ElseBranch::If(statement) => shift_statement(statement, offset),
                }
            }
        }
        StmtKind::While { condition, body } => {
            shift_expr(condition, offset);
            shift_block(body, offset);
        }
        StmtKind::For { iterable, body, .. } => {
            shift_expr(iterable, offset);
            shift_block(body, offset);
        }
        StmtKind::Break | StmtKind::Continue => {}
        StmtKind::Expr(expr) => shift_expr(expr, offset),
    }
}

fn shift_expr(expr: &mut Expr, offset: usize) {
    shift_span(&mut expr.span, offset);
    match &mut expr.kind {
        ExprKind::Array(items) => {
            for item in items {
                shift_expr(item, offset);
            }
        }
        ExprKind::CollectionLiteral { items, collection } => {
            shift_type(collection, offset);
            for item in items {
                shift_expr(item, offset);
            }
        }
        ExprKind::Call { callee, args } => {
            shift_expr(callee, offset);
            for arg in args {
                shift_expr(arg, offset);
            }
        }
        ExprKind::Member { object, .. } => shift_expr(object, offset),
        ExprKind::GenericMember { object, args, .. } => {
            shift_expr(object, offset);
            for arg in args {
                shift_type(arg, offset);
            }
        }
        ExprKind::GenericType { args, .. } => {
            for arg in args {
                shift_type(arg, offset);
            }
        }
        ExprKind::StructInit { args, fields, .. } => {
            for arg in args {
                shift_type(arg, offset);
            }
            for field in fields {
                shift_span(&mut field.span, offset);
                shift_expr(&mut field.value, offset);
            }
        }
        ExprKind::Range { start, end, .. } => {
            shift_expr(start, offset);
            shift_expr(end, offset);
        }
        ExprKind::Binary { left, right, .. } => {
            shift_expr(left, offset);
            shift_expr(right, offset);
        }
        ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
            shift_expr(operand, offset);
        }
        ExprKind::Match { value, branches } => {
            shift_expr(value, offset);
            for branch in branches {
                shift_span(&mut branch.span, offset);
                shift_pattern(&mut branch.pattern, offset);
                match &mut branch.body {
                    MatchBranchBody::Expr(expr) => shift_expr(expr, offset),
                    MatchBranchBody::Block(block) => shift_block(block, offset),
                }
            }
        }
        ExprKind::Lambda(function) => shift_function(function, offset),
        ExprKind::Identifier(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn shift_pattern(pattern: &mut Pattern, offset: usize) {
    match pattern {
        Pattern::Variant { payload, span, .. } => {
            shift_span(span, offset);
            if let Some(payload) = payload {
                shift_pattern(payload, offset);
            }
        }
        Pattern::Struct { fields, span, .. } => {
            shift_span(span, offset);
            for field in fields {
                shift_span(&mut field.span, offset);
                shift_pattern(&mut field.pattern, offset);
            }
        }
        Pattern::Binding { span, .. }
        | Pattern::String { span, .. }
        | Pattern::Number { span, .. }
        | Pattern::Range { span, .. }
        | Pattern::Wildcard { span } => shift_span(span, offset),
    }
}

fn shift_span(span: &mut Span, offset: usize) {
    span.start += offset;
    span.end += offset;
}
