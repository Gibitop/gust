// Statement lowering helpers carry the current local scope plus shared executable type/function
// tables; splitting that state here would make this pipeline less direct than its call sites.
#[allow(clippy::too_many_arguments)]
fn lower_if_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    let StmtKind::If {
        condition,
        then_branch,
        else_branch,
    } = &statement.kind
    else {
        return None;
    };

    let condition = lower_expr(
        condition,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        Some(LoweredType::Basic(BasicType::Bool)),
        "expected supported `if` condition in executable builds",
    )?;
    let then_branch = lower_conditional_block(
        then_branch,
        &mut locals.clone(),
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        return_type,
    );
    let else_branch = else_branch.as_ref().map(|else_branch| {
        let mut branch_locals = locals.clone();

        match else_branch {
            ElseBranch::Block(block) => lower_conditional_block(
                block,
                &mut branch_locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                return_type,
            ),
            ElseBranch::If(statement) => lower_if_statement(
                statement,
                &branch_locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                return_type,
            )
            .into_iter()
            .collect(),
        }
    });

    Some(LoweredStatement::If {
        condition,
        then_branch,
        else_branch,
    })
}

// Conditional blocks need mutable local scope, shared lowering tables, diagnostics, and the
// expected return type so branch-local bindings and returns are handled consistently.
#[allow(clippy::too_many_arguments)]
fn lower_conditional_block(
    block: &Block,
    locals: &mut HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Vec<LoweredStatement> {
    let mut statements = Vec::new();

    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { .. } => {
                if let Some(statement) = lower_local_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Assign { .. } => {
                if let Some(statement) = lower_assignment_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Return { value } => {
                let Some(return_type) = return_type else {
                    diagnostics.push(Diagnostic::error(
                        statement.span,
                        "return statements are not supported in executable builds",
                    ));
                    continue;
                };
                let value = value.as_ref().and_then(|value| {
                    lower_expr(
                        value,
                        locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(return_type.clone()),
                        "expected supported return value in executable builds",
                    )
                });
                statements.push(LoweredStatement::Return(value));
            }
            StmtKind::If { .. } => {
                if let Some(statement) = lower_if_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    return_type,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::For { .. } => statements.extend(lower_for_statement(
                statement,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                return_type,
            )),
            StmtKind::While { .. } => {
                if let Some(statement) = lower_while_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    return_type,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Break => statements.push(LoweredStatement::Break),
            StmtKind::Continue => statements.push(LoweredStatement::Continue),
            StmtKind::Expr(expr) => {
                if let Some(statement) = lower_expression_statement(
                    expr,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    return_type,
                ) {
                    statements.push(statement);
                }
            }
        }
    }

    statements
}

// Expression statements share the statement-lowering environment so calls, matches, increments,
// and returns all report diagnostics against the same executable context.
#[allow(clippy::too_many_arguments)]
fn lower_expression_statement(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    if matches!(expr.kind, ExprKind::Match { .. }) {
        return lower_match_statement(
            expr,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            return_type,
        );
    }

    if matches!(expr.kind, ExprKind::PostfixIncrement(_)) {
        return lower_expr(
            expr,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            None,
            "expected supported increment expression in executable builds",
        )
        .map(LoweredStatement::Expr);
    }

    let ExprKind::Call { callee, args } = &expr.kind else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            "only function calls are supported as expression statements in executable builds",
        ));
        return None;
    };

    let is_io_println = match &callee.kind {
        ExprKind::Member { object, name } if name == "println" => {
            matches!(&object.kind, ExprKind::Identifier(name) if name == "io")
        }
        _ => false,
    };

    if is_io_println {
        if args.len() != 1 {
            diagnostics.push(Diagnostic::error(
                expr.span,
                "`io.println` expects exactly one `string` value in executable builds",
            ));
            return None;
        }

        let value = lower_expr(
            &args[0],
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            None,
            "`io.println` only accepts `string` values in executable builds",
        )?;

        if value.type_ != LoweredType::Basic(BasicType::String) {
            diagnostics.push(Diagnostic::error(
                args[0].span,
                format!(
                    "`io.println` only accepts `string` values in executable builds, got `{}`",
                    value.type_.name()
                ),
            ));
            return None;
        }

        return Some(LoweredStatement::Println(value));
    }

    lower_expr(
        expr,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        None,
        "expected supported function call in executable builds",
    )
    .map(LoweredStatement::Expr)
}

// Match statements need both pattern-specific local mutation and the shared lowering tables used
// by nested branch statements, so the full context stays explicit at this boundary.
#[allow(clippy::too_many_arguments)]
fn lower_match_statement(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    let ExprKind::Match { value, branches } = &expr.kind else {
        return None;
    };
    let value_mutable = expression_has_mutable_capability(value, locals);
    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        None,
        "expected supported match value in executable builds",
    )?;
    if !match_value_type_is_supported(&value.type_) {
        diagnostics.push(Diagnostic::error(
            expr.span,
            "match statements require an enum, struct, `string`, `bool`, or integer value in executable builds",
        ));
        return None;
    }

    let mut lowered_branches = Vec::new();
    let temp_name = match_temp_name(expr.span);
    for branch in branches {
        let mut branch_locals = locals.clone();
        let pattern = lower_match_pattern(
            &branch.pattern,
            &value.type_,
            value_mutable,
            &mut branch_locals,
            enums,
            structs,
            diagnostics,
            &temp_name,
        )?;
        let statements = match &branch.body {
            MatchBranchBody::Block(block) => lower_conditional_block(
                block,
                &mut branch_locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                return_type,
            ),
            MatchBranchBody::Expr(branch_expr) => lower_expression_statement(
                branch_expr,
                &branch_locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                return_type,
            )
            .into_iter()
            .collect(),
        };
        lowered_branches.push(LoweredMatchStatementBranch {
            pattern,
            statements,
        });
    }

    Some(LoweredStatement::Match {
        value,
        temp_name,
        branches: lowered_branches,
    })
}

// Block-bodied match expression branches lower setup statements and the final value together,
// which requires both statement context and the expression's expected result type.
#[allow(clippy::too_many_arguments)]
fn lower_match_expression_branch_block(
    block: &Block,
    locals: &mut HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<LoweredType>,
) -> Option<(Vec<LoweredStatement>, LoweredExpr)> {
    let Some((last_statement, setup_statements)) = block.statements.split_last() else {
        diagnostics.push(Diagnostic::error(
            block.span,
            "block-bodied match expression branches must return a value",
        ));
        return None;
    };
    let StmtKind::Return { value: Some(value) } = &last_statement.kind else {
        diagnostics.push(Diagnostic::error(
            last_statement.span,
            "block-bodied match expression branches must end with `return value`",
        ));
        return None;
    };

    let mut statements = Vec::new();
    for statement in setup_statements {
        match &statement.kind {
            StmtKind::Let { .. } => {
                if let Some(statement) = lower_local_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Assign { .. } => {
                if let Some(statement) = lower_assignment_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::If { .. } => {
                if let Some(statement) = lower_if_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::While { .. } => {
                if let Some(statement) = lower_while_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Expr(expr) => {
                if let Some(statement) = lower_expression_statement(
                    expr,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    diagnostics,
                    None,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Return { .. } => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "return statements are only supported as the final value of block-bodied match expression branches",
                ));
            }
            StmtKind::Break => statements.push(LoweredStatement::Break),
            StmtKind::Continue => statements.push(LoweredStatement::Continue),
            StmtKind::For { .. } => statements.extend(lower_for_statement(
                statement,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                None,
            )),
        }
    }

    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        expected_type,
        "expected supported match branch value in executable builds",
    )?;

    Some((statements, value))
}

fn lower_local_statement(
    statement: &Stmt,
    locals: &mut HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredStatement> {
    let StmtKind::Let {
        name,
        mutable,
        type_annotation,
        value,
    } = &statement.kind
    else {
        return None;
    };

    let mut can_lower = true;

    let annotated_type = if let Some(type_annotation) = type_annotation {
        let self_type = locals.get("Self").map(|local| &local.type_);
        let lowered = lower_value_type_ref_in_context(
            type_annotation,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "only basic, struct, enum, trait, and function local types are supported in executable builds",
        );
        if lowered.is_none() {
            can_lower = false;
        }
        lowered
    } else {
        None
    };

    let value = if let Some(value) = value {
        lower_expr(
            value,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            annotated_type.clone(),
            "only literal, string concat, struct literal, field access, and function call local values are supported in executable builds",
        )
    } else if let Some(type_) = annotated_type.clone() {
        let kind = match type_ {
            LoweredType::Basic(BasicType::String) => LoweredExprKind::StringLiteral(String::new()),
            LoweredType::Basic(BasicType::Char) => LoweredExprKind::NumberLiteral("0".to_string()),
            LoweredType::Basic(BasicType::Bool) => LoweredExprKind::BoolLiteral(false),
            LoweredType::Basic(type_) if type_.is_numeric() => {
                LoweredExprKind::NumberLiteral("0".to_string())
            }
            LoweredType::Struct(_)
            | LoweredType::Enum(_)
            | LoweredType::Trait(_)
            | LoweredType::Function { .. } => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "struct, enum, trait, and function locals must include an initializer in executable builds",
                ));
                return None;
            }
            LoweredType::Void => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "`void` locals are not supported in executable builds",
                ));
                return None;
            }
            LoweredType::Basic(_) => unreachable!("all basic types have default values"),
        };

        Some(LoweredExpr { type_, kind })
    } else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            "let declarations without values must include a type annotation",
        ));
        None
    };

    let value = value?;

    if !can_lower {
        return None;
    }

    if *mutable
        && matches!(value.type_, LoweredType::Struct(_))
        && !lowered_expression_has_mutable_capability(&value, locals, signatures, structs)
    {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!(
                "cannot initialize mutable binding `{name}` from an immutable value; use `.clone()` to create an independent mutable object"
            ),
        ));
        return None;
    }

    let captured = CAPTURED_NAMES.with(|names| names.borrow().contains(name));

    if locals
        .insert(
            name.clone(),
            LoweringLocal {
                type_: value.type_.clone(),
                mutable: *mutable,
                replacement: None,
                captured,
            },
        )
        .is_some()
    {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("duplicate local `{name}` in executable build"),
        ));
        return None;
    }

    if captured {
        Some(LoweredStatement::LocalCell {
            name: name.clone(),
            value,
        })
    } else {
        Some(LoweredStatement::Local {
            name: name.clone(),
            value,
        })
    }
}

fn lower_assignment_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredStatement> {
    let StmtKind::Assign { target, op, value } = &statement.kind else {
        return None;
    };
    let binding_name = match &target.kind {
        ExprKind::Identifier(name) => name,
        ExprKind::Member { object, .. } => {
            let Some(name) = mutable_member_root(object) else {
                diagnostics.push(Diagnostic::error(
                    target.span,
                    "field assignment target must be rooted in a mutable local struct binding in executable builds",
                ));
                return None;
            };
            name
        }
        _ => {
            diagnostics.push(Diagnostic::error(
                target.span,
                "assignment target must be a mutable local binding in executable builds",
            ));
            return None;
        }
    };
    let Some(local) = locals.get(binding_name) else {
        diagnostics.push(Diagnostic::error(
            target.span,
            format!("unknown local `{binding_name}` in executable build"),
        ));
        return None;
    };

    if !local.mutable {
        let message = if matches!(target.kind, ExprKind::Member { .. }) {
            format!("cannot mutate field of immutable binding `{binding_name}` in executable build")
        } else {
            format!("cannot assign to immutable binding `{binding_name}` in executable build")
        };
        diagnostics.push(Diagnostic::error(target.span, message));
        return None;
    }

    let lowered_target = lower_expr(
        target,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        None,
        "expected supported assignment target in executable builds",
    )?;

    let compound_value;
    let value = if let Some(op) = op {
        compound_value = Expr {
            kind: ExprKind::Binary {
                left: Box::new(target.clone()),
                op: *op,
                right: Box::new(value.clone()),
            },
            span: statement.span,
        };
        &compound_value
    } else {
        value
    };
    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        Some(lowered_target.type_.clone()),
        "expected supported assignment value in executable builds",
    )?;

    Some(LoweredStatement::Assignment {
        target: lowered_target,
        value,
    })
}
