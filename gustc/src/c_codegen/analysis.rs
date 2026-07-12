fn program_uses_type(program: &LoweredProgram, type_: BasicType) -> bool {
    program
        .structs
        .iter()
        .any(|struct_| struct_uses_type(struct_, type_))
        || program.enums.iter().any(|enum_| {
            enum_
                .variants
                .iter()
                .any(|variant| variant.payload == Some(LoweredType::Basic(type_)))
        })
        || program
            .functions
            .iter()
            .any(|function| function_uses_type(function, type_))
        || program
            .closure_functions
            .iter()
            .any(|function| closure_function_uses_type(function, type_))
        || program
            .statements
            .iter()
            .any(|statement| statement_uses_type(statement, type_))
}

fn struct_uses_type(struct_: &LoweredStruct, type_: BasicType) -> bool {
    struct_
        .fields
        .iter()
        .any(|field| field.type_ == LoweredType::Basic(type_))
}

fn function_uses_type(function: &LoweredFunction, type_: BasicType) -> bool {
    function.return_type == LoweredType::Basic(type_)
        || function
            .params
            .iter()
            .any(|param| param.type_ == LoweredType::Basic(type_))
        || function
            .statements
            .iter()
            .any(|statement| statement_uses_type(statement, type_))
        || expr_uses_type(&function.return_value, type_)
}

fn closure_function_uses_type(function: &LoweredClosureFunction, type_: BasicType) -> bool {
    function.return_type == LoweredType::Basic(type_)
        || function
            .params
            .iter()
            .any(|param| param.type_ == LoweredType::Basic(type_))
        || function
            .captures
            .iter()
            .any(|capture| capture.type_ == LoweredType::Basic(type_))
        || function
            .statements
            .iter()
            .any(|statement| statement_uses_type(statement, type_))
        || expr_uses_type(&function.return_value, type_)
}

fn statement_uses_type(statement: &LoweredStatement, type_: BasicType) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Println(value)
        | LoweredStatement::Expr(value) => expr_uses_type(value, type_),
        LoweredStatement::Assignment { target, value } => {
            expr_uses_type(target, type_) || expr_uses_type(value, type_)
        }
        LoweredStatement::Return(value) => value
            .as_ref()
            .is_some_and(|value| expr_uses_type(value, type_)),
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_uses_type(condition, type_)
                || then_branch
                    .iter()
                    .any(|statement| statement_uses_type(statement, type_))
                || else_branch.as_ref().is_some_and(|statements| {
                    statements
                        .iter()
                        .any(|statement| statement_uses_type(statement, type_))
                })
        }
        LoweredStatement::While { condition, body } => {
            expr_uses_type(condition, type_)
                || body
                    .iter()
                    .any(|statement| statement_uses_type(statement, type_))
        }
        LoweredStatement::Break | LoweredStatement::Continue => false,
        LoweredStatement::Match {
            value, branches, ..
        } => {
            expr_uses_type(value, type_)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_uses_type(guard, type_))
                        || branch
                        .statements
                        .iter()
                        .any(|statement| statement_uses_type(statement, type_))
                })
        }
    }
}

fn expr_uses_type(expr: &LoweredExpr, type_: BasicType) -> bool {
    expr.type_ == LoweredType::Basic(type_)
        || match &expr.kind {
            LoweredExprKind::StringConcat(left, right) => {
                expr_uses_type(left, type_) || expr_uses_type(right, type_)
            }
            LoweredExprKind::PostfixIncrement(operand)
            | LoweredExprKind::Not(operand)
            | LoweredExprKind::Negate(operand) => expr_uses_type(operand, type_),
            LoweredExprKind::Logical { left, right, .. }
            | LoweredExprKind::Arithmetic { left, right, .. }
            | LoweredExprKind::Comparison { left, right, .. } => {
                expr_uses_type(left, type_) || expr_uses_type(right, type_)
            }
            LoweredExprKind::StructLiteral { fields, .. } => fields
                .iter()
                .any(|field| expr_uses_type(&field.value, type_)),
            LoweredExprKind::EnumLiteral { payload, .. } => payload
                .as_ref()
                .is_some_and(|payload| expr_uses_type(payload, type_)),
            LoweredExprKind::EnumPayload { object, .. } => expr_uses_type(object, type_),
            LoweredExprKind::MatchPatternBinding {
                matched_value,
                alternatives,
            } => {
                expr_uses_type(matched_value, type_)
                    || alternatives
                        .iter()
                        .any(|alternative| expr_uses_type(&alternative.value, type_))
            }
            LoweredExprKind::Match {
                value, branches, ..
            } => {
                expr_uses_type(value, type_)
                    || branches.iter().any(|branch| {
                        branch
                            .guard
                            .as_ref()
                            .is_some_and(|guard| expr_uses_type(guard, type_))
                            || branch
                            .statements
                            .iter()
                            .any(|statement| statement_uses_type(statement, type_))
                            || expr_uses_type(&branch.value, type_)
                    })
            }
            LoweredExprKind::FieldAccess { object, .. }
            | LoweredExprKind::TraitObject { value: object, .. }
            | LoweredExprKind::Clone(object)
            | LoweredExprKind::NumberToString(object) => expr_uses_type(object, type_),
            LoweredExprKind::Call { args, .. } => args.iter().any(|arg| expr_uses_type(arg, type_)),
            LoweredExprKind::CollectionLiteral { items, .. } => {
                items.iter().any(|item| expr_uses_type(item, type_))
            }
            LoweredExprKind::IndirectCall { callee, args }
            | LoweredExprKind::DynamicCall {
                object: callee,
                args,
                ..
            } => expr_uses_type(callee, type_) || args.iter().any(|arg| expr_uses_type(arg, type_)),
            LoweredExprKind::Void
            | LoweredExprKind::StringLiteral(_)
            | LoweredExprKind::BoolLiteral(_)
            | LoweredExprKind::NumberLiteral(_)
            | LoweredExprKind::Local(_)
            | LoweredExprKind::LocalCell(_)
            | LoweredExprKind::CapturedLocal { .. }
            | LoweredExprKind::Closure { .. }
            | LoweredExprKind::MatchValue(_) => false,
        }
}

fn program_uses_fixed_width_int(program: &LoweredProgram) -> bool {
    [
        BasicType::U8,
        BasicType::Char,
        BasicType::U16,
        BasicType::U32,
        BasicType::U64,
        BasicType::I8,
        BasicType::I16,
        BasicType::I32,
        BasicType::I64,
    ]
    .into_iter()
    .any(|type_| program_uses_type(program, type_))
}

fn number_to_string_types(program: &LoweredProgram) -> Vec<BasicType> {
    [
        BasicType::U8,
        BasicType::U16,
        BasicType::U32,
        BasicType::U64,
        BasicType::U128,
        BasicType::Usize,
        BasicType::I8,
        BasicType::I16,
        BasicType::I32,
        BasicType::I64,
        BasicType::I128,
        BasicType::F32,
        BasicType::F64,
    ]
    .into_iter()
    .filter(|type_| program_uses_number_to_string(program, *type_))
    .collect()
}

fn program_uses_number_to_string(program: &LoweredProgram, type_: BasicType) -> bool {
    program.functions.iter().any(|function| {
        function
            .statements
            .iter()
            .any(|statement| statement_uses_number_to_string(statement, type_))
            || expr_uses_number_to_string(&function.return_value, type_)
    }) || program.closure_functions.iter().any(|function| {
        function
            .statements
            .iter()
            .any(|statement| statement_uses_number_to_string(statement, type_))
            || expr_uses_number_to_string(&function.return_value, type_)
    }) || program
        .statements
        .iter()
        .any(|statement| statement_uses_number_to_string(statement, type_))
}

fn statement_uses_number_to_string(statement: &LoweredStatement, type_: BasicType) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Println(value)
        | LoweredStatement::Expr(value) => expr_uses_number_to_string(value, type_),
        LoweredStatement::Assignment { target, value } => {
            expr_uses_number_to_string(target, type_) || expr_uses_number_to_string(value, type_)
        }
        LoweredStatement::Return(value) => value
            .as_ref()
            .is_some_and(|value| expr_uses_number_to_string(value, type_)),
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_uses_number_to_string(condition, type_)
                || then_branch
                    .iter()
                    .any(|statement| statement_uses_number_to_string(statement, type_))
                || else_branch.as_ref().is_some_and(|statements| {
                    statements
                        .iter()
                        .any(|statement| statement_uses_number_to_string(statement, type_))
                })
        }
        LoweredStatement::While { condition, body } => {
            expr_uses_number_to_string(condition, type_)
                || body
                    .iter()
                    .any(|statement| statement_uses_number_to_string(statement, type_))
        }
        LoweredStatement::Break | LoweredStatement::Continue => false,
        LoweredStatement::Match {
            value, branches, ..
        } => {
            expr_uses_number_to_string(value, type_)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_uses_number_to_string(guard, type_))
                        || branch
                        .statements
                        .iter()
                        .any(|statement| statement_uses_number_to_string(statement, type_))
                })
        }
    }
}

fn expr_uses_number_to_string(expr: &LoweredExpr, type_: BasicType) -> bool {
    match &expr.kind {
        LoweredExprKind::NumberToString(object) => {
            object.type_ == LoweredType::Basic(type_) || expr_uses_number_to_string(object, type_)
        }
        LoweredExprKind::StringConcat(left, right)
        | LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Arithmetic { left, right, .. }
        | LoweredExprKind::Comparison { left, right, .. } => {
            expr_uses_number_to_string(left, type_) || expr_uses_number_to_string(right, type_)
        }
        LoweredExprKind::PostfixIncrement(operand)
        | LoweredExprKind::Not(operand)
        | LoweredExprKind::Negate(operand)
        | LoweredExprKind::EnumPayload {
            object: operand, ..
        }
        | LoweredExprKind::FieldAccess {
            object: operand, ..
        }
        | LoweredExprKind::TraitObject { value: operand, .. }
        | LoweredExprKind::Clone(operand) => expr_uses_number_to_string(operand, type_),
        LoweredExprKind::StructLiteral { fields, .. } => fields
            .iter()
            .any(|field| expr_uses_number_to_string(&field.value, type_)),
        LoweredExprKind::EnumLiteral { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| expr_uses_number_to_string(payload, type_)),
        LoweredExprKind::MatchPatternBinding {
            matched_value,
            alternatives,
        } => {
            expr_uses_number_to_string(matched_value, type_)
                || alternatives
                    .iter()
                    .any(|alternative| expr_uses_number_to_string(&alternative.value, type_))
        }
        LoweredExprKind::Match {
            value, branches, ..
            } => {
                expr_uses_number_to_string(value, type_)
                    || branches.iter().any(|branch| {
                        branch
                            .guard
                            .as_ref()
                            .is_some_and(|guard| expr_uses_number_to_string(guard, type_))
                            || branch
                            .statements
                            .iter()
                            .any(|statement| statement_uses_number_to_string(statement, type_))
                            || expr_uses_number_to_string(&branch.value, type_)
                    })
        }
        LoweredExprKind::Call { args, .. } => args
            .iter()
            .any(|arg| expr_uses_number_to_string(arg, type_)),
        LoweredExprKind::CollectionLiteral { items, .. } => items
            .iter()
            .any(|item| expr_uses_number_to_string(item, type_)),
        LoweredExprKind::IndirectCall { callee, args }
        | LoweredExprKind::DynamicCall {
            object: callee,
            args,
            ..
        } => {
            expr_uses_number_to_string(callee, type_)
                || args
                    .iter()
                    .any(|arg| expr_uses_number_to_string(arg, type_))
        }
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. }
        | LoweredExprKind::MatchValue(_) => false,
    }
}

fn program_uses_string_concat(program: &LoweredProgram) -> bool {
    program
        .functions
        .iter()
        .any(function_uses_string_concat)
        || program
            .closure_functions
            .iter()
            .any(closure_function_uses_string_concat)
        || program.statements.iter().any(statement_uses_string_concat)
}

fn program_uses_string_equality(program: &LoweredProgram) -> bool {
    program.functions.iter().any(|function| {
        function
            .statements
            .iter()
            .any(statement_uses_string_equality)
            || expr_uses_string_equality(&function.return_value)
    }) || program.closure_functions.iter().any(|function| {
        function
            .statements
            .iter()
            .any(statement_uses_string_equality)
            || expr_uses_string_equality(&function.return_value)
    }) || program
        .statements
        .iter()
        .any(statement_uses_string_equality)
}

fn statement_uses_string_equality(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Println(value)
        | LoweredStatement::Expr(value) => expr_uses_string_equality(value),
        LoweredStatement::Assignment { target, value } => {
            expr_uses_string_equality(target) || expr_uses_string_equality(value)
        }
        LoweredStatement::Return(value) => value.as_ref().is_some_and(expr_uses_string_equality),
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_uses_string_equality(condition)
                || then_branch.iter().any(statement_uses_string_equality)
                || else_branch
                    .as_ref()
                    .is_some_and(|statements| statements.iter().any(statement_uses_string_equality))
        }
        LoweredStatement::While { condition, body } => {
            expr_uses_string_equality(condition) || body.iter().any(statement_uses_string_equality)
        }
        LoweredStatement::Break | LoweredStatement::Continue => false,
        LoweredStatement::Match {
            value, branches, ..
        } => {
            expr_uses_string_equality(value)
                || branches.iter().any(|branch| {
                    lowered_pattern_uses_string_equality(&branch.pattern)
                        || branch
                            .guard
                            .as_ref()
                            .is_some_and(expr_uses_string_equality)
                        || branch.statements.iter().any(statement_uses_string_equality)
                })
        }
    }
}

fn expr_uses_string_equality(expr: &LoweredExpr) -> bool {
    match &expr.kind {
        LoweredExprKind::PostfixIncrement(operand)
        | LoweredExprKind::Not(operand)
        | LoweredExprKind::Negate(operand) => expr_uses_string_equality(operand),
        LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Arithmetic { left, right, .. } => {
            expr_uses_string_equality(left) || expr_uses_string_equality(right)
        }
        LoweredExprKind::Comparison { left, op, right } => {
            matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
                && left.type_ == LoweredType::Basic(BasicType::String)
                || expr_uses_string_equality(left)
                || expr_uses_string_equality(right)
        }
        LoweredExprKind::StringConcat(left, right) => {
            expr_uses_string_equality(left) || expr_uses_string_equality(right)
        }
        LoweredExprKind::StructLiteral { fields, .. } => fields
            .iter()
            .any(|field| expr_uses_string_equality(&field.value)),
        LoweredExprKind::EnumLiteral { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| expr_uses_string_equality(payload)),
        LoweredExprKind::EnumPayload { object, .. } => expr_uses_string_equality(object),
        LoweredExprKind::MatchPatternBinding {
            matched_value,
            alternatives,
        } => {
            expr_uses_string_equality(matched_value)
                || alternatives.iter().any(|alternative| {
                    lowered_pattern_uses_string_equality(&alternative.pattern)
                        || expr_uses_string_equality(&alternative.value)
                })
        }
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_string_equality(value)
                || branches.iter().any(|branch| {
                    lowered_pattern_uses_string_equality(&branch.pattern)
                        || branch
                            .guard
                            .as_ref()
                            .is_some_and(expr_uses_string_equality)
                        || branch.statements.iter().any(statement_uses_string_equality)
                        || expr_uses_string_equality(&branch.value)
                })
        }
        LoweredExprKind::FieldAccess { object, .. }
        | LoweredExprKind::TraitObject { value: object, .. }
        | LoweredExprKind::Clone(object)
        | LoweredExprKind::NumberToString(object) => expr_uses_string_equality(object),
        LoweredExprKind::Call { args, .. } => args.iter().any(expr_uses_string_equality),
        LoweredExprKind::CollectionLiteral { items, .. } => {
            items.iter().any(expr_uses_string_equality)
        }
        LoweredExprKind::IndirectCall { callee, args }
        | LoweredExprKind::DynamicCall {
            object: callee,
            args,
            ..
        } => expr_uses_string_equality(callee) || args.iter().any(expr_uses_string_equality),
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. }
        | LoweredExprKind::MatchValue(_) => false,
    }
}

fn lowered_pattern_uses_string_equality(pattern: &LoweredPattern) -> bool {
    match pattern {
        LoweredPattern::Or(alternatives) => alternatives
            .iter()
            .any(lowered_pattern_uses_string_equality),
        LoweredPattern::Variant { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| lowered_pattern_uses_string_equality(payload)),
        LoweredPattern::Struct { fields, .. } => fields
            .iter()
            .any(|field| lowered_pattern_uses_string_equality(&field.pattern)),
        LoweredPattern::String(_) => true,
        LoweredPattern::Bool(_)
        | LoweredPattern::Number { .. }
        | LoweredPattern::Range { .. }
        | LoweredPattern::Wildcard => false,
    }
}

fn function_uses_string_concat(function: &LoweredFunction) -> bool {
    function.statements.iter().any(statement_uses_string_concat)
        || expr_uses_string_concat(&function.return_value)
}

fn closure_function_uses_string_concat(function: &LoweredClosureFunction) -> bool {
    function.statements.iter().any(statement_uses_string_concat)
        || expr_uses_string_concat(&function.return_value)
}

fn statement_uses_string_concat(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Println(value)
        | LoweredStatement::Expr(value) => expr_uses_string_concat(value),
        LoweredStatement::Assignment { target, value } => {
            expr_uses_string_concat(target) || expr_uses_string_concat(value)
        }
        LoweredStatement::Return(value) => value.as_ref().is_some_and(expr_uses_string_concat),
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_uses_string_concat(condition)
                || then_branch.iter().any(statement_uses_string_concat)
                || else_branch
                    .as_ref()
                    .is_some_and(|statements| statements.iter().any(statement_uses_string_concat))
        }
        LoweredStatement::While { condition, body } => {
            expr_uses_string_concat(condition) || body.iter().any(statement_uses_string_concat)
        }
        LoweredStatement::Break | LoweredStatement::Continue => false,
        LoweredStatement::Match {
            value, branches, ..
        } => {
            expr_uses_string_concat(value)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(expr_uses_string_concat)
                        || branch.statements.iter().any(statement_uses_string_concat)
                })
        }
    }
}

fn expr_uses_string_concat(expr: &LoweredExpr) -> bool {
    match &expr.kind {
        LoweredExprKind::StringConcat(_, _) => true,
        LoweredExprKind::PostfixIncrement(operand)
        | LoweredExprKind::Not(operand)
        | LoweredExprKind::Negate(operand) => expr_uses_string_concat(operand),
        LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Arithmetic { left, right, .. }
        | LoweredExprKind::Comparison { left, right, .. } => {
            expr_uses_string_concat(left) || expr_uses_string_concat(right)
        }
        LoweredExprKind::StructLiteral { fields, .. } => fields
            .iter()
            .any(|field| expr_uses_string_concat(&field.value)),
        LoweredExprKind::EnumLiteral { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| expr_uses_string_concat(payload)),
        LoweredExprKind::EnumPayload { object, .. } => expr_uses_string_concat(object),
        LoweredExprKind::MatchPatternBinding {
            matched_value,
            alternatives,
        } => {
            expr_uses_string_concat(matched_value)
                || alternatives
                    .iter()
                    .any(|alternative| expr_uses_string_concat(&alternative.value))
        }
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_string_concat(value)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(expr_uses_string_concat)
                        || branch.statements.iter().any(statement_uses_string_concat)
                        || expr_uses_string_concat(&branch.value)
                })
        }
        LoweredExprKind::FieldAccess { object, .. }
        | LoweredExprKind::TraitObject { value: object, .. }
        | LoweredExprKind::Clone(object)
        | LoweredExprKind::NumberToString(object) => expr_uses_string_concat(object),
        LoweredExprKind::Call { args, .. } => args.iter().any(expr_uses_string_concat),
        LoweredExprKind::CollectionLiteral { items, .. } => {
            items.iter().any(expr_uses_string_concat)
        }
        LoweredExprKind::IndirectCall { callee, args }
        | LoweredExprKind::DynamicCall {
            object: callee,
            args,
            ..
        } => expr_uses_string_concat(callee) || args.iter().any(expr_uses_string_concat),
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. }
        | LoweredExprKind::MatchValue(_) => false,
    }
}

fn program_uses_enum_trait_object(program: &LoweredProgram) -> bool {
    program
        .statements
        .iter()
        .any(statement_uses_enum_trait_object)
        || program
            .functions
            .iter()
            .any(function_uses_enum_trait_object)
        || program
            .closure_functions
            .iter()
            .any(closure_function_uses_enum_trait_object)
}

fn function_uses_enum_trait_object(function: &LoweredFunction) -> bool {
    function
        .statements
        .iter()
        .any(statement_uses_enum_trait_object)
        || expr_uses_enum_trait_object(&function.return_value)
}

fn closure_function_uses_enum_trait_object(function: &LoweredClosureFunction) -> bool {
    function
        .statements
        .iter()
        .any(statement_uses_enum_trait_object)
        || expr_uses_enum_trait_object(&function.return_value)
}

fn statement_uses_enum_trait_object(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Println(value)
        | LoweredStatement::Expr(value) => expr_uses_enum_trait_object(value),
        LoweredStatement::Assignment { target, value } => {
            expr_uses_enum_trait_object(target) || expr_uses_enum_trait_object(value)
        }
        LoweredStatement::Return(value) => value.as_ref().is_some_and(expr_uses_enum_trait_object),
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_uses_enum_trait_object(condition)
                || then_branch.iter().any(statement_uses_enum_trait_object)
                || else_branch.as_ref().is_some_and(|statements| {
                    statements.iter().any(statement_uses_enum_trait_object)
                })
        }
        LoweredStatement::While { condition, body } => {
            expr_uses_enum_trait_object(condition)
                || body.iter().any(statement_uses_enum_trait_object)
        }
        LoweredStatement::Break | LoweredStatement::Continue => false,
        LoweredStatement::Match {
            value, branches, ..
        } => {
            expr_uses_enum_trait_object(value)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(expr_uses_enum_trait_object)
                        || branch
                        .statements
                        .iter()
                        .any(statement_uses_enum_trait_object)
                })
        }
    }
}

fn expr_uses_enum_trait_object(expr: &LoweredExpr) -> bool {
    match &expr.kind {
        LoweredExprKind::TraitObject {
            self_type: LoweredType::Enum(_),
            ..
        } => true,
        LoweredExprKind::TraitObject { value, .. } => expr_uses_enum_trait_object(value),
        LoweredExprKind::PostfixIncrement(operand)
        | LoweredExprKind::Not(operand)
        | LoweredExprKind::Negate(operand)
        | LoweredExprKind::EnumPayload {
            object: operand, ..
        }
        | LoweredExprKind::FieldAccess {
            object: operand, ..
        }
        | LoweredExprKind::Clone(operand)
        | LoweredExprKind::NumberToString(operand) => expr_uses_enum_trait_object(operand),
        LoweredExprKind::StringConcat(left, right)
        | LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Arithmetic { left, right, .. }
        | LoweredExprKind::Comparison { left, right, .. } => {
            expr_uses_enum_trait_object(left) || expr_uses_enum_trait_object(right)
        }
        LoweredExprKind::StructLiteral { fields, .. } => fields
            .iter()
            .any(|field| expr_uses_enum_trait_object(&field.value)),
        LoweredExprKind::EnumLiteral { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| expr_uses_enum_trait_object(payload)),
        LoweredExprKind::MatchPatternBinding {
            matched_value,
            alternatives,
        } => {
            expr_uses_enum_trait_object(matched_value)
                || alternatives
                    .iter()
                    .any(|alternative| expr_uses_enum_trait_object(&alternative.value))
        }
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_enum_trait_object(value)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(expr_uses_enum_trait_object)
                        || branch
                        .statements
                        .iter()
                        .any(statement_uses_enum_trait_object)
                        || expr_uses_enum_trait_object(&branch.value)
                })
        }
        LoweredExprKind::Call { args, .. } => args.iter().any(expr_uses_enum_trait_object),
        LoweredExprKind::CollectionLiteral { items, .. } => {
            items.iter().any(expr_uses_enum_trait_object)
        }
        LoweredExprKind::IndirectCall { callee, args }
        | LoweredExprKind::DynamicCall {
            object: callee,
            args,
            ..
        } => expr_uses_enum_trait_object(callee) || args.iter().any(expr_uses_enum_trait_object),
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. }
        | LoweredExprKind::MatchValue(_) => false,
    }
}

fn statement_uses_println(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Println(_) => true,
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_uses_println(condition)
                || then_branch.iter().any(statement_uses_println)
                || else_branch
                    .as_ref()
                    .is_some_and(|statements| statements.iter().any(statement_uses_println))
        }
        LoweredStatement::Match {
            value, branches, ..
        } => {
            expr_uses_println(value)
                || branches.iter().any(|branch| {
                    branch.guard.as_ref().is_some_and(expr_uses_println)
                        || branch.statements.iter().any(statement_uses_println)
                })
        }
        LoweredStatement::While { condition, body } => {
            expr_uses_println(condition) || body.iter().any(statement_uses_println)
        }
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Expr(value) => expr_uses_println(value),
        LoweredStatement::Assignment { target, value } => {
            expr_uses_println(target) || expr_uses_println(value)
        }
        LoweredStatement::Return(value) => value.as_ref().is_some_and(expr_uses_println),
        LoweredStatement::Break | LoweredStatement::Continue => false,
    }
}

fn expr_uses_println(expr: &LoweredExpr) -> bool {
    match &expr.kind {
        LoweredExprKind::StringConcat(left, right)
        | LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Arithmetic { left, right, .. }
        | LoweredExprKind::Comparison { left, right, .. } => {
            expr_uses_println(left) || expr_uses_println(right)
        }
        LoweredExprKind::PostfixIncrement(operand)
        | LoweredExprKind::Not(operand)
        | LoweredExprKind::Negate(operand)
        | LoweredExprKind::EnumPayload {
            object: operand, ..
        }
        | LoweredExprKind::FieldAccess {
            object: operand, ..
        }
        | LoweredExprKind::TraitObject { value: operand, .. }
        | LoweredExprKind::Clone(operand)
        | LoweredExprKind::NumberToString(operand) => expr_uses_println(operand),
        LoweredExprKind::StructLiteral { fields, .. } => {
            fields.iter().any(|field| expr_uses_println(&field.value))
        }
        LoweredExprKind::EnumLiteral { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| expr_uses_println(payload)),
        LoweredExprKind::MatchPatternBinding {
            matched_value,
            alternatives,
        } => {
            expr_uses_println(matched_value)
                || alternatives
                    .iter()
                    .any(|alternative| expr_uses_println(&alternative.value))
        }
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_println(value)
                || branches.iter().any(|branch| {
                    branch.guard.as_ref().is_some_and(expr_uses_println)
                        || branch.statements.iter().any(statement_uses_println)
                        || expr_uses_println(&branch.value)
                })
        }
        LoweredExprKind::Call { args, .. } => args.iter().any(expr_uses_println),
        LoweredExprKind::CollectionLiteral { items, .. } => items.iter().any(expr_uses_println),
        LoweredExprKind::IndirectCall { callee, args }
        | LoweredExprKind::DynamicCall {
            object: callee,
            args,
            ..
        } => expr_uses_println(callee) || args.iter().any(expr_uses_println),
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. }
        | LoweredExprKind::MatchValue(_) => false,
    }
}

fn ordered_functions(functions: &[LoweredFunction]) -> Vec<&LoweredFunction> {
    fn visit<'a>(
        index: usize,
        functions: &'a [LoweredFunction],
        states: &mut [u8],
        ordered: &mut Vec<&'a LoweredFunction>,
    ) {
        if states[index] != 0 {
            return;
        }

        states[index] = 1;

        for (dependency_index, dependency) in functions.iter().enumerate() {
            if dependency_index != index && function_calls_name(&functions[index], &dependency.name)
            {
                visit(dependency_index, functions, states, ordered);
            }
        }

        states[index] = 2;
        ordered.push(&functions[index]);
    }

    let mut states = vec![0; functions.len()];
    let mut ordered = Vec::new();

    for index in 0..functions.len() {
        visit(index, functions, &mut states, &mut ordered);
    }

    ordered
}

fn function_calls_name(function: &LoweredFunction, name: &str) -> bool {
    function
        .statements
        .iter()
        .any(|statement| statement_calls_name(statement, name))
        || expr_calls_name(&function.return_value, name)
}

fn statement_calls_name(statement: &LoweredStatement, name: &str) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Println(value)
        | LoweredStatement::Expr(value) => expr_calls_name(value, name),
        LoweredStatement::Assignment { target, value } => {
            expr_calls_name(target, name) || expr_calls_name(value, name)
        }
        LoweredStatement::Return(value) => value
            .as_ref()
            .is_some_and(|value| expr_calls_name(value, name)),
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_calls_name(condition, name)
                || then_branch
                    .iter()
                    .any(|statement| statement_calls_name(statement, name))
                || else_branch.as_ref().is_some_and(|statements| {
                    statements
                        .iter()
                        .any(|statement| statement_calls_name(statement, name))
                })
        }
        LoweredStatement::While { condition, body } => {
            expr_calls_name(condition, name)
                || body
                    .iter()
                    .any(|statement| statement_calls_name(statement, name))
        }
        LoweredStatement::Break | LoweredStatement::Continue => false,
        LoweredStatement::Match {
            value, branches, ..
        } => {
            expr_calls_name(value, name)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_calls_name(guard, name))
                        || branch
                        .statements
                        .iter()
                        .any(|statement| statement_calls_name(statement, name))
                })
        }
    }
}

fn expr_calls_name(expr: &LoweredExpr, name: &str) -> bool {
    match &expr.kind {
        LoweredExprKind::StringConcat(left, right) => {
            expr_calls_name(left, name) || expr_calls_name(right, name)
        }
        LoweredExprKind::PostfixIncrement(operand)
        | LoweredExprKind::Not(operand)
        | LoweredExprKind::Negate(operand) => expr_calls_name(operand, name),
        LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Arithmetic { left, right, .. }
        | LoweredExprKind::Comparison { left, right, .. } => {
            expr_calls_name(left, name) || expr_calls_name(right, name)
        }
        LoweredExprKind::StructLiteral { fields, .. } => fields
            .iter()
            .any(|field| expr_calls_name(&field.value, name)),
        LoweredExprKind::EnumLiteral { payload, .. } => payload
            .as_ref()
            .is_some_and(|payload| expr_calls_name(payload, name)),
        LoweredExprKind::EnumPayload { object, .. } => expr_calls_name(object, name),
        LoweredExprKind::MatchPatternBinding {
            matched_value,
            alternatives,
        } => {
            expr_calls_name(matched_value, name)
                || alternatives
                    .iter()
                    .any(|alternative| expr_calls_name(&alternative.value, name))
        }
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_calls_name(value, name)
                || branches.iter().any(|branch| {
                    branch
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_calls_name(guard, name))
                        || branch
                        .statements
                        .iter()
                        .any(|statement| statement_calls_name(statement, name))
                        || expr_calls_name(&branch.value, name)
                })
        }
        LoweredExprKind::FieldAccess { object, .. }
        | LoweredExprKind::TraitObject { value: object, .. }
        | LoweredExprKind::Clone(object)
        | LoweredExprKind::NumberToString(object) => expr_calls_name(object, name),
        LoweredExprKind::Call {
            name: called_name,
            args,
        } => called_name == name || args.iter().any(|arg| expr_calls_name(arg, name)),
        LoweredExprKind::CollectionLiteral {
            constructor,
            add,
            items,
        } => {
            constructor == name
                || add == name
                || items.iter().any(|item| expr_calls_name(item, name))
        }
        LoweredExprKind::IndirectCall { callee, args } => {
            expr_calls_name(callee, name) || args.iter().any(|arg| expr_calls_name(arg, name))
        }
        LoweredExprKind::DynamicCall { object, args, .. } => {
            expr_calls_name(object, name) || args.iter().any(|arg| expr_calls_name(arg, name))
        }
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. }
        | LoweredExprKind::MatchValue(_) => false,
    }
}
