fn infer_function_return_type(
    function: &FunctionDecl,
    self_type: Option<&LoweredType>,
    has_self: bool,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Result<Option<LoweredType>, ReturnTypeConflict> {
    let mut locals = HashMap::new();

    if let Some(self_type) = self_type {
        locals.insert("Self".to_string(), self_type.clone());
        if has_self {
            locals.insert("self".to_string(), self_type.clone());
        }
    }

    for param in function
        .params
        .iter()
        .filter(|param| !has_self || !is_self_param(param))
    {
        let Some(type_ref) = param.type_ref.as_ref() else {
            return Ok(None);
        };
        let Some(type_) = quiet_lower_type_ref(type_ref, &locals, structs, enums, traits) else {
            return Ok(None);
        };
        locals.insert(param.name.clone(), type_);
    }

    match &function.body {
        FunctionBody::Expr(expr) => Ok(infer_expr_type(
            expr, &locals, signatures, structs, enums, traits,
        )),
        FunctionBody::Block(block) => {
            let mut return_type = None;
            let mut has_unresolved_value_return = false;
            infer_block_return_types(
                block,
                &mut locals,
                signatures,
                structs,
                enums,
                traits,
                &mut return_type,
                &mut has_unresolved_value_return,
            )?;

            if return_type.is_none() && has_unresolved_value_return {
                Ok(None)
            } else {
                Ok(Some(return_type.unwrap_or(LoweredType::Void)))
            }
        }
    }
}

// Return inference walks blocks with a cloned local type scope plus shared function/type tables
// and mutable aggregate return state, mirroring the lowering pass it supports.
#[allow(clippy::too_many_arguments)]
fn infer_block_return_types(
    block: &Block,
    locals: &mut HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    return_type: &mut Option<LoweredType>,
    has_unresolved_value_return: &mut bool,
) -> Result<(), ReturnTypeConflict> {
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { name, value, .. } => {
                if let Some(value) = value
                    && let Some(type_) =
                        infer_expr_type(value, locals, signatures, structs, enums, traits)
                {
                    locals.insert(name.clone(), type_);
                }
            }
            StmtKind::Return { value: Some(value) } => {
                if let Some(type_) =
                    infer_expr_type(value, locals, signatures, structs, enums, traits)
                {
                    merge_inferred_return_type(return_type, type_, value.span)?;
                } else {
                    *has_unresolved_value_return = true;
                }
            }
            StmtKind::Return { value: None } => {
                merge_inferred_return_type(return_type, LoweredType::Void, statement.span)?;
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut branch_locals = locals.clone();
                infer_block_return_types(
                    then_branch,
                    &mut branch_locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    return_type,
                    has_unresolved_value_return,
                )?;

                if let Some(else_branch) = else_branch {
                    let mut branch_locals = locals.clone();

                    match else_branch {
                        ElseBranch::Block(block) => infer_block_return_types(
                            block,
                            &mut branch_locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            return_type,
                            has_unresolved_value_return,
                        )?,
                        ElseBranch::If(statement) => {
                            let block = Block {
                                statements: vec![(**statement).clone()],
                                span: statement.span,
                            };
                            infer_block_return_types(
                                &block,
                                &mut branch_locals,
                                signatures,
                                structs,
                                enums,
                                traits,
                                return_type,
                                has_unresolved_value_return,
                            )?;
                        }
                    }
                }
            }
            StmtKind::While { body, .. } => {
                infer_block_return_types(
                    body,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    return_type,
                    has_unresolved_value_return,
                )?;
            }
            StmtKind::Assign { .. }
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::For { .. }
            | StmtKind::Expr(_) => {}
        }
    }

    Ok(())
}

struct ReturnTypeConflict {
    span: Span,
    first: LoweredType,
    second: LoweredType,
}

fn merge_inferred_return_type(
    return_type: &mut Option<LoweredType>,
    next_type: LoweredType,
    span: Span,
) -> Result<(), ReturnTypeConflict> {
    let Some(current_type) = return_type else {
        *return_type = Some(next_type);
        return Ok(());
    };

    if *current_type == next_type {
        return Ok(());
    }

    Err(ReturnTypeConflict {
        span,
        first: current_type.clone(),
        second: next_type,
    })
}

fn lowered_statement_always_returns_value(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Return(Some(_)) => true,
        LoweredStatement::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            then_branch
                .iter()
                .any(lowered_statement_always_returns_value)
                && else_branch
                    .iter()
                    .any(lowered_statement_always_returns_value)
        }
        LoweredStatement::Local { .. }
        | LoweredStatement::LocalCell { .. }
        | LoweredStatement::Assignment { .. }
        | LoweredStatement::Println(_)
        | LoweredStatement::Panic { .. }
        | LoweredStatement::Expr(_)
        | LoweredStatement::Return(None)
        | LoweredStatement::While { .. }
        | LoweredStatement::Break
        | LoweredStatement::Continue
        | LoweredStatement::Match { .. }
        | LoweredStatement::If {
            else_branch: None, ..
        } => false,
    }
}
