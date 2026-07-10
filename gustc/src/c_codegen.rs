use std::collections::HashSet;

use crate::ast::{BasicType, BinaryOp};
use crate::lower::{
    LoweredClosureFunction, LoweredEnum, LoweredExpr, LoweredExprKind, LoweredFunction,
    LoweredPattern, LoweredProgram, LoweredStatement, LoweredStruct, LoweredType,
};

pub fn emit_c(program: &LoweredProgram) -> String {
    let uses_string_equality = program_uses_string_equality(program);
    let number_to_string_types = number_to_string_types(program);
    let uses_bool = program_uses_type(program, BasicType::Bool)
        || uses_string_equality
        || number_to_string_types.contains(&BasicType::I128);
    let uses_usize = program_uses_type(program, BasicType::Usize)
        || program
            .structs
            .iter()
            .any(|struct_| struct_.raw_buffer_element.is_some());
    let uses_float =
        program_uses_type(program, BasicType::F32) || program_uses_type(program, BasicType::F64);
    let uses_fixed_width_int = program_uses_fixed_width_int(program);
    let uses_string_concat = program_uses_string_concat(program);
    let uses_number_to_string = !number_to_string_types.is_empty();
    let uses_enum_trait_object = program_uses_enum_trait_object(program);
    let uses_println = program.statements.iter().any(statement_uses_println)
        || program
            .functions
            .iter()
            .any(|function| function.statements.iter().any(statement_uses_println))
        || program
            .closure_functions
            .iter()
            .any(|function| function.statements.iter().any(statement_uses_println));

    let mut source = String::new();

    if uses_bool {
        source.push_str("#include <stdbool.h>\n");
    }

    if uses_usize {
        source.push_str("#include <stddef.h>\n");
    }

    if uses_fixed_width_int {
        source.push_str("#include <stdint.h>\n");
    }

    if uses_println || uses_number_to_string {
        source.push_str("#include <stdio.h>\n");
    }

    if uses_float {
        source.push_str("#include <math.h>\n");
    }

    let uses_alloc = uses_string_concat
        || uses_number_to_string
        || uses_enum_trait_object
        || !program.structs.is_empty()
        || !program.closure_functions.is_empty();

    if uses_alloc {
        source.push_str("#include <stdlib.h>\n#include <string.h>\n");
    } else if uses_string_equality {
        source.push_str("#include <string.h>\n");
    }

    if !source.is_empty() {
        source.push('\n');
    }

    push_c_type_definitions(&mut source, program);
    push_c_function_type_definitions(&mut source, program);

    if uses_alloc {
        source.push_str("static void* gust_rt_alloc(size_t size) {\n");
        source.push_str("    return malloc(size);\n");
        source.push_str("}\n\n");
    }

    if uses_string_concat {
        source.push_str(
            "static char* gust_rt_string_concat(const char* left, const char* right) {\n",
        );
        source.push_str("    size_t left_len = strlen(left);\n");
        source.push_str("    size_t right_len = strlen(right);\n");
        source.push_str("    char* result = gust_rt_alloc(left_len + right_len + 1);\n");
        source.push_str("    memcpy(result, left, left_len);\n");
        source.push_str("    memcpy(result + left_len, right, right_len + 1);\n");
        source.push_str("    return result;\n");
        source.push_str("}\n\n");
    }

    for type_ in number_to_string_types {
        push_c_number_to_string_helper(&mut source, type_);
    }

    if uses_string_equality {
        source
            .push_str("static bool gust_rt_string_equal(const char* left, const char* right) {\n");
        source.push_str("    return strcmp(left, right) == 0;\n");
        source.push_str("}\n\n");
    }

    if uses_println {
        source.push_str("static void gust_rt_io_println(const char* value) {\n");
        source.push_str("    puts(value);\n");
        source.push_str("}\n\n");
    }

    push_c_struct_runtime_helpers(&mut source, program);
    push_c_closure_env_structs(&mut source, &program.closure_functions);

    for function in &program.closure_functions {
        push_c_closure_function_signature(&mut source, function);
        source.push_str(";\n\n");
    }

    push_c_trait_dispatch_helpers(&mut source, program);

    for function in ordered_functions(&program.functions) {
        if function_calls_name(function, &function.name) {
            push_c_function_signature(&mut source, function);
            source.push_str(";\n\n");
        }

        push_c_function(&mut source, function, &program.structs);
        source.push('\n');
    }

    for function in &program.closure_functions {
        push_c_closure_function(&mut source, function, &program.structs);
        source.push('\n');
    }

    source.push_str("int main(void) {\n");

    for statement in &program.statements {
        push_c_statement(&mut source, statement, 1, &program.structs);
    }

    source.push_str("    return 0;\n}\n");
    source
}

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
            LoweredExprKind::Match {
                value, branches, ..
            } => {
                expr_uses_type(value, type_)
                    || branches.iter().any(|branch| {
                        branch
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
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_number_to_string(value, type_)
                || branches.iter().any(|branch| {
                    branch
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
        .any(|function| function_uses_string_concat(function))
        || program
            .closure_functions
            .iter()
            .any(|function| closure_function_uses_string_concat(function))
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
                    matches!(branch.pattern, LoweredPattern::String(_))
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
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_string_equality(value)
                || branches.iter().any(|branch| {
                    matches!(branch.pattern, LoweredPattern::String(_))
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
                || branches
                    .iter()
                    .any(|branch| branch.statements.iter().any(statement_uses_string_concat))
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
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_string_concat(value)
                || branches.iter().any(|branch| {
                    branch.statements.iter().any(statement_uses_string_concat)
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
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_enum_trait_object(value)
                || branches.iter().any(|branch| {
                    branch
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
                || branches
                    .iter()
                    .any(|branch| branch.statements.iter().any(statement_uses_println))
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
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_uses_println(value)
                || branches.iter().any(|branch| {
                    branch.statements.iter().any(statement_uses_println)
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
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            expr_calls_name(value, name)
                || branches.iter().any(|branch| {
                    branch
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

fn push_c_struct(source: &mut String, struct_: &LoweredStruct) {
    source.push_str("// Gust struct: ");
    source.push_str(&struct_.name);
    source.push('\n');
    source.push_str("struct ");
    push_c_struct_name(source, &struct_.name);
    source.push_str(" {\n");

    for field in &struct_.fields {
        source.push_str("    ");
        push_c_type(source, &field.type_);
        source.push(' ');
        push_c_local_name(source, &field.name);
        source.push_str(";\n");
    }

    if struct_.raw_buffer_element.is_some() {
        source.push_str("    void* gust_data;\n");
        source.push_str("    size_t gust_capacity;\n");
        source.push_str("    size_t gust_length;\n");
    }

    source.push_str("};\n");
}

fn raw_buffer_element_type<'a>(
    structs: &'a [LoweredStruct],
    type_: &LoweredType,
) -> Option<&'a LoweredType> {
    let LoweredType::Struct(name) = type_ else {
        return None;
    };
    structs
        .iter()
        .find(|struct_| struct_.name == *name)
        .and_then(|struct_| struct_.raw_buffer_element.as_ref())
}

fn raw_buffer_method(name: &str) -> Option<&str> {
    ["withCapacity", "capacity", "read", "write", "clear", "grow"]
        .into_iter()
        .find(|method| name.ends_with(&format!(".{method}")))
}

fn push_c_type_definitions(source: &mut String, program: &LoweredProgram) {
    let mut emitted = HashSet::new();
    let mut remaining = program.structs.len() + program.enums.len();

    for trait_ in &program.traits {
        source.push_str("typedef struct ");
        push_c_trait_vtable_name(source, &trait_.name);
        source.push(' ');
        push_c_trait_vtable_name(source, &trait_.name);
        source.push_str(";\n");
        source.push_str("typedef struct ");
        push_c_trait_name(source, &trait_.name);
        source.push_str(" {\n");
        source.push_str("    void* gust_self;\n");
        source.push_str("    const ");
        push_c_trait_vtable_name(source, &trait_.name);
        source.push_str("* gust_vtable;\n");
        source.push_str("} ");
        push_c_trait_name(source, &trait_.name);
        source.push_str(";\n");
    }

    if !program.traits.is_empty() {
        source.push('\n');
    }

    for struct_ in &program.structs {
        source.push_str("typedef struct ");
        push_c_struct_name(source, &struct_.name);
        source.push(' ');
        push_c_struct_name(source, &struct_.name);
        source.push_str(";\n");
    }

    if !program.structs.is_empty() {
        source.push('\n');
    }

    while remaining > 0 {
        let previous_remaining = remaining;

        for struct_ in &program.structs {
            let key = format!("struct:{}", struct_.name);

            if emitted.contains(&key)
                || !struct_
                    .fields
                    .iter()
                    .all(|field| type_definition_is_emitted(&field.type_, &emitted))
            {
                continue;
            }

            push_c_struct(source, struct_);
            source.push('\n');
            emitted.insert(key);
            remaining -= 1;
        }

        for enum_ in &program.enums {
            let key = format!("enum:{}", enum_.name);

            if emitted.contains(&key)
                || !enum_.variants.iter().all(|variant| {
                    variant
                        .payload
                        .as_ref()
                        .is_none_or(|payload| type_definition_is_emitted(payload, &emitted))
                })
            {
                continue;
            }

            push_c_enum(source, enum_);
            source.push('\n');
            emitted.insert(key);
            remaining -= 1;
        }

        if remaining == previous_remaining {
            break;
        }
    }

    for trait_ in &program.traits {
        source.push_str("struct ");
        push_c_trait_vtable_name(source, &trait_.name);
        source.push_str(" {\n");
        for method in &trait_.methods {
            source.push_str("    ");
            push_c_type(source, &method.return_type);
            source.push_str(" (*");
            push_c_trait_method_field_name(source, &method.name);
            source.push_str(")(void*");
            for param in &method.params {
                source.push_str(", ");
                push_c_type(source, &param.type_);
            }
            source.push_str(");\n");
        }
        source.push_str("};\n\n");
    }
}

fn push_c_function_type_definitions(source: &mut String, program: &LoweredProgram) {
    let mut types = Vec::new();
    collect_program_function_types(program, &mut types);
    types.sort_by_key(type_name_key);
    types.dedup();

    for type_ in types {
        let LoweredType::Function {
            params,
            return_type,
        } = &type_
        else {
            continue;
        };

        source.push_str("typedef struct ");
        push_c_function_type_name(source, &type_);
        source.push_str(" {\n");
        source.push_str("    void* gust_env;\n");
        source.push_str("    ");
        push_c_type(source, return_type);
        source.push_str(" (*gust_call)(void*");
        for param in params {
            source.push_str(", ");
            push_c_type(source, &param.type_);
        }
        source.push_str(");\n} ");
        push_c_function_type_name(source, &type_);
        source.push_str(";\n\n");
    }
}

fn collect_program_function_types(program: &LoweredProgram, types: &mut Vec<LoweredType>) {
    for struct_ in &program.structs {
        for field in &struct_.fields {
            collect_function_type(&field.type_, types);
        }
    }
    for enum_ in &program.enums {
        for variant in &enum_.variants {
            if let Some(payload) = &variant.payload {
                collect_function_type(payload, types);
            }
        }
    }
    for function in &program.functions {
        collect_function_type(&function.return_type, types);
        for param in &function.params {
            collect_function_type(&param.type_, types);
        }
        for statement in &function.statements {
            collect_statement_function_types(statement, types);
        }
        collect_expr_function_types(&function.return_value, types);
    }
    for function in &program.closure_functions {
        collect_function_type(&function.return_type, types);
        for param in &function.params {
            collect_function_type(&param.type_, types);
        }
        for capture in &function.captures {
            collect_function_type(&capture.type_, types);
        }
        for statement in &function.statements {
            collect_statement_function_types(statement, types);
        }
        collect_expr_function_types(&function.return_value, types);
    }
    for statement in &program.statements {
        collect_statement_function_types(statement, types);
    }
}

fn collect_function_type(type_: &LoweredType, types: &mut Vec<LoweredType>) {
    if let LoweredType::Function {
        params,
        return_type,
    } = type_
    {
        types.push(type_.clone());
        for param in params {
            collect_function_type(&param.type_, types);
        }
        collect_function_type(return_type, types);
    }
}

fn collect_statement_function_types(statement: &LoweredStatement, types: &mut Vec<LoweredType>) {
    match statement {
        LoweredStatement::Local { value, .. }
        | LoweredStatement::LocalCell { value, .. }
        | LoweredStatement::Println(value)
        | LoweredStatement::Expr(value) => collect_expr_function_types(value, types),
        LoweredStatement::Assignment { target, value } => {
            collect_expr_function_types(target, types);
            collect_expr_function_types(value, types);
        }
        LoweredStatement::Return(value) => {
            if let Some(value) = value {
                collect_expr_function_types(value, types);
            }
        }
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr_function_types(condition, types);
            for statement in then_branch {
                collect_statement_function_types(statement, types);
            }
            if let Some(else_branch) = else_branch {
                for statement in else_branch {
                    collect_statement_function_types(statement, types);
                }
            }
        }
        LoweredStatement::While { condition, body } => {
            collect_expr_function_types(condition, types);
            for statement in body {
                collect_statement_function_types(statement, types);
            }
        }
        LoweredStatement::Match {
            value, branches, ..
        } => {
            collect_expr_function_types(value, types);
            for branch in branches {
                for statement in &branch.statements {
                    collect_statement_function_types(statement, types);
                }
            }
        }
        LoweredStatement::Break | LoweredStatement::Continue => {}
    }
}

fn collect_expr_function_types(expr: &LoweredExpr, types: &mut Vec<LoweredType>) {
    collect_function_type(&expr.type_, types);
    match &expr.kind {
        LoweredExprKind::StringConcat(left, right)
        | LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Arithmetic { left, right, .. }
        | LoweredExprKind::Comparison { left, right, .. } => {
            collect_expr_function_types(left, types);
            collect_expr_function_types(right, types);
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
        | LoweredExprKind::NumberToString(operand) => collect_expr_function_types(operand, types),
        LoweredExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                collect_expr_function_types(&field.value, types);
            }
        }
        LoweredExprKind::EnumLiteral { payload, .. } => {
            if let Some(payload) = payload {
                collect_expr_function_types(payload, types);
            }
        }
        LoweredExprKind::Match {
            value, branches, ..
        } => {
            collect_expr_function_types(value, types);
            for branch in branches {
                for statement in &branch.statements {
                    collect_statement_function_types(statement, types);
                }
                collect_expr_function_types(&branch.value, types);
            }
        }
        LoweredExprKind::Call { args, .. } => {
            for arg in args {
                collect_expr_function_types(arg, types);
            }
        }
        LoweredExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_expr_function_types(item, types);
            }
        }
        LoweredExprKind::IndirectCall { callee, args } => {
            collect_expr_function_types(callee, types);
            for arg in args {
                collect_expr_function_types(arg, types);
            }
        }
        LoweredExprKind::DynamicCall { object, args, .. } => {
            collect_expr_function_types(object, types);
            for arg in args {
                collect_expr_function_types(arg, types);
            }
        }
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. }
        | LoweredExprKind::MatchValue(_) => {}
    }
}

fn type_definition_is_emitted(type_: &LoweredType, emitted: &HashSet<String>) -> bool {
    match type_ {
        LoweredType::Basic(_)
        | LoweredType::Struct(_)
        | LoweredType::Trait(_)
        | LoweredType::Function { .. }
        | LoweredType::Void => true,
        LoweredType::Enum(name) => emitted.contains(&format!("enum:{name}")),
    }
}

fn push_c_struct_runtime_helpers(source: &mut String, program: &LoweredProgram) {
    if program.structs.is_empty() {
        return;
    }

    source.push_str("typedef struct gust_rt_clone_entry {\n");
    source.push_str("    const void* gust_source;\n");
    source.push_str("    void* gust_clone;\n");
    source.push_str("    struct gust_rt_clone_entry* gust_next;\n");
    source.push_str("} gust_rt_clone_entry;\n\n");
    source.push_str(
        "static void* gust_rt_clone_lookup(gust_rt_clone_entry* entries, const void* source) {\n",
    );
    source.push_str("    for (; entries != NULL; entries = entries->gust_next) {\n");
    source.push_str("        if (entries->gust_source == source) {\n");
    source.push_str("            return entries->gust_clone;\n");
    source.push_str("        }\n");
    source.push_str("    }\n");
    source.push_str("    return NULL;\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_clone_register(gust_rt_clone_entry** entries, const void* source, void* clone) {\n");
    source
        .push_str("    gust_rt_clone_entry* entry = gust_rt_alloc(sizeof(gust_rt_clone_entry));\n");
    source.push_str("    entry->gust_source = source;\n");
    source.push_str("    entry->gust_clone = clone;\n");
    source.push_str("    entry->gust_next = *entries;\n");
    source.push_str("    *entries = entry;\n");
    source.push_str("}\n\n");

    for struct_ in &program.structs {
        source.push_str("static ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* ");
        push_c_struct_clone_internal_name(source, &struct_.name);
        source.push_str("(const ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* value, gust_rt_clone_entry** entries);\n");
    }
    for enum_ in &program.enums {
        source.push_str("static ");
        push_c_enum_name(source, &enum_.name);
        source.push(' ');
        push_c_enum_clone_internal_name(source, &enum_.name);
        source.push('(');
        push_c_enum_name(source, &enum_.name);
        source.push_str(" value, gust_rt_clone_entry** entries);\n");
    }
    source.push('\n');

    for enum_ in &program.enums {
        source.push_str("static ");
        push_c_enum_name(source, &enum_.name);
        source.push(' ');
        push_c_enum_clone_internal_name(source, &enum_.name);
        source.push('(');
        push_c_enum_name(source, &enum_.name);
        source.push_str(" value, gust_rt_clone_entry** entries) {\n");
        source.push_str("    ");
        push_c_enum_name(source, &enum_.name);
        source.push_str(" result = value;\n");
        source.push_str("    switch (value.gust_tag) {\n");

        for variant in &enum_.variants {
            source.push_str("        case ");
            push_c_enum_variant_tag(source, &enum_.name, &variant.name);
            source.push_str(":\n");
            match &variant.payload {
                Some(LoweredType::Struct(name)) => {
                    source.push_str("            result.gust_payload.");
                    push_c_local_name(source, &variant.name);
                    source.push_str(" = ");
                    push_c_struct_clone_internal_name(source, name);
                    source.push_str("(value.gust_payload.");
                    push_c_local_name(source, &variant.name);
                    source.push_str(", entries);\n");
                }
                Some(LoweredType::Enum(name)) => {
                    source.push_str("            result.gust_payload.");
                    push_c_local_name(source, &variant.name);
                    source.push_str(" = ");
                    push_c_enum_clone_internal_name(source, name);
                    source.push_str("(value.gust_payload.");
                    push_c_local_name(source, &variant.name);
                    source.push_str(", entries);\n");
                }
                Some(LoweredType::Basic(_))
                | Some(LoweredType::Trait(_))
                | Some(LoweredType::Function { .. })
                | None => {}
                Some(LoweredType::Void) => {
                    unreachable!("enum variants cannot contain void")
                }
            }
            source.push_str("            break;\n");
        }

        source.push_str("    }\n");
        source.push_str("    return result;\n");
        source.push_str("}\n\n");
    }

    for struct_ in &program.structs {
        source.push_str("static ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* ");
        push_c_struct_new_name(source, &struct_.name);
        source.push('(');
        for (index, field) in struct_.fields.iter().enumerate() {
            if index > 0 {
                source.push_str(", ");
            }

            push_c_type(source, &field.type_);
            source.push(' ');
            push_c_local_name(source, &field.name);
        }
        source.push_str(") {\n    ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* result = gust_rt_alloc(sizeof(");
        push_c_struct_name(source, &struct_.name);
        source.push_str("));\n");
        for field in &struct_.fields {
            source.push_str("    result->");
            push_c_local_name(source, &field.name);
            source.push_str(" = ");
            push_c_local_name(source, &field.name);
            source.push_str(";\n");
        }
        if struct_.raw_buffer_element.is_some() {
            source.push_str("    result->gust_data = NULL;\n");
            source.push_str("    result->gust_capacity = 0;\n");
            source.push_str("    result->gust_length = 0;\n");
        }
        source.push_str("    return result;\n");
        source.push_str("}\n\n");

        source.push_str("static ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* ");
        push_c_struct_clone_internal_name(source, &struct_.name);
        source.push_str("(const ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* value, gust_rt_clone_entry** entries) {\n");
        source.push_str("    if (value == NULL) {\n        return NULL;\n    }\n");
        source.push_str("    void* existing = gust_rt_clone_lookup(*entries, value);\n");
        source.push_str("    if (existing != NULL) {\n        return existing;\n    }\n    ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* result = gust_rt_alloc(sizeof(");
        push_c_struct_name(source, &struct_.name);
        source.push_str("));\n");
        source.push_str("    gust_rt_clone_register(entries, value, result);\n");

        for field in &struct_.fields {
            source.push_str("    result->");
            push_c_local_name(source, &field.name);
            source.push_str(" = ");
            if let LoweredType::Struct(name) = &field.type_ {
                push_c_struct_clone_internal_name(source, name);
                source.push_str("(value->");
                push_c_local_name(source, &field.name);
                source.push_str(", entries)");
            } else if let LoweredType::Enum(name) = &field.type_ {
                push_c_enum_clone_internal_name(source, name);
                source.push_str("(value->");
                push_c_local_name(source, &field.name);
                source.push_str(", entries)");
            } else {
                source.push_str("value->");
                push_c_local_name(source, &field.name);
            }
            source.push_str(";\n");
        }

        if let Some(element) = &struct_.raw_buffer_element {
            source.push_str("    result->gust_capacity = value->gust_capacity;\n");
            source.push_str("    result->gust_length = value->gust_length;\n");
            source.push_str("    result->gust_data = NULL;\n");
            source.push_str("    if (value->gust_capacity > 0) {\n        result->gust_data = gust_rt_alloc(sizeof(");
            push_c_type(source, element);
            source.push_str(") * value->gust_capacity);\n    }\n");
            source.push_str("    for (size_t gust_index = 0; gust_index < value->gust_length; gust_index++) {\n        ((");
            push_c_type(source, element);
            source.push_str("*)result->gust_data)[gust_index] = ");
            push_c_raw_buffer_clone_element(source, element);
            source.push_str(";\n    }\n");
        }

        source.push_str("    return result;\n");
        source.push_str("}\n\n");
        source.push_str("static ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* ");
        push_c_struct_clone_name(source, &struct_.name);
        source.push_str("(const ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* value) {\n");
        source.push_str("    gust_rt_clone_entry* entries = NULL;\n");
        source.push_str("    return ");
        push_c_struct_clone_internal_name(source, &struct_.name);
        source.push_str("(value, &entries);\n");
        source.push_str("}\n\n");
    }
}

fn push_c_raw_buffer_clone_element(source: &mut String, type_: &LoweredType) {
    match type_ {
        LoweredType::Struct(name) => {
            push_c_struct_clone_internal_name(source, name);
            source.push_str("(((");
            push_c_type(source, type_);
            source.push_str("*)value->gust_data)[gust_index], entries)");
        }
        LoweredType::Enum(name) => {
            push_c_enum_clone_internal_name(source, name);
            source.push_str("(((");
            push_c_type(source, type_);
            source.push_str("*)value->gust_data)[gust_index], entries)");
        }
        LoweredType::Basic(_) | LoweredType::Trait(_) | LoweredType::Function { .. } => {
            source.push_str("((");
            push_c_type(source, type_);
            source.push_str("*)value->gust_data)[gust_index]");
        }
        LoweredType::Void => unreachable!("raw buffers cannot contain void"),
    }
}

fn push_c_enum(source: &mut String, enum_: &LoweredEnum) {
    source.push_str("// Gust enum: ");
    source.push_str(&enum_.name);
    source.push('\n');
    source.push_str("typedef enum ");
    push_c_enum_tag_name(source, &enum_.name);
    source.push_str(" {\n");

    for variant in &enum_.variants {
        source.push_str("    ");
        push_c_enum_variant_tag(source, &enum_.name, &variant.name);
        source.push_str(",\n");
    }

    source.push_str("} ");
    push_c_enum_tag_name(source, &enum_.name);
    source.push_str(";\n");
    source.push_str("typedef struct ");
    push_c_enum_name(source, &enum_.name);
    source.push_str(" {\n    ");
    push_c_enum_tag_name(source, &enum_.name);
    source.push_str(" gust_tag;\n");

    if enum_
        .variants
        .iter()
        .any(|variant| variant.payload.is_some())
    {
        source.push_str("    union {\n");

        for variant in &enum_.variants {
            let Some(payload) = &variant.payload else {
                continue;
            };

            source.push_str("        ");
            push_c_type(source, payload);
            source.push(' ');
            push_c_local_name(source, &variant.name);
            source.push_str(";\n");
        }

        source.push_str("    } gust_payload;\n");
    }

    source.push_str("} ");
    push_c_enum_name(source, &enum_.name);
    source.push_str(";\n");
}

fn push_c_function(source: &mut String, function: &LoweredFunction, structs: &[LoweredStruct]) {
    source.push_str("// Gust function: ");
    source.push_str(&function.name);
    source.push('\n');
    push_c_function_signature(source, function);
    source.push_str(" {\n");

    for statement in &function.statements {
        push_c_statement(source, statement, 1, structs);
    }

    if function.return_type != LoweredType::Void && function.return_value.type_ != LoweredType::Void
    {
        source.push_str("    return ");
        push_c_value(source, &function.return_value, structs);
        source.push_str(";\n");
    }

    source.push_str("}\n");
}

fn push_c_function_signature(source: &mut String, function: &LoweredFunction) {
    source.push_str("static ");
    push_c_type(source, &function.return_type);
    source.push(' ');
    push_c_function_name(source, &function.name);
    source.push('(');

    for (index, param) in function.params.iter().enumerate() {
        if index > 0 {
            source.push_str(", ");
        }

        push_c_type(source, &param.type_);
        source.push(' ');
        push_c_local_name(source, &param.name);
    }

    source.push(')');
}

fn push_c_closure_env_structs(source: &mut String, functions: &[LoweredClosureFunction]) {
    for function in functions {
        if function.captures.is_empty() {
            continue;
        }

        source.push_str("typedef struct ");
        source.push_str(&closure_env_type_name(&function.name));
        source.push_str(" {\n");
        for capture in &function.captures {
            source.push_str("    ");
            push_c_type(source, &capture.type_);
            source.push_str("* ");
            push_c_local_name(source, &capture.name);
            source.push_str(";\n");
        }
        source.push_str("} ");
        source.push_str(&closure_env_type_name(&function.name));
        source.push_str(";\n\n");
    }
}

fn push_c_closure_function(
    source: &mut String,
    function: &LoweredClosureFunction,
    structs: &[LoweredStruct],
) {
    source.push_str("// Gust closure: ");
    source.push_str(&function.name);
    source.push('\n');
    push_c_closure_function_signature(source, function);
    source.push_str(" {\n");
    if !function.captures.is_empty() {
        source.push_str("    ");
        source.push_str(&closure_env_type_name(&function.name));
        source.push_str("* gust_env = gust_raw_env;\n");
    } else {
        source.push_str("    (void)gust_raw_env;\n");
    }

    for statement in &function.statements {
        push_c_statement(source, statement, 1, structs);
    }

    if function.return_type != LoweredType::Void && function.return_value.type_ != LoweredType::Void
    {
        source.push_str("    return ");
        push_c_value(source, &function.return_value, structs);
        source.push_str(";\n");
    }

    source.push_str("}\n");
}

fn push_c_closure_function_signature(source: &mut String, function: &LoweredClosureFunction) {
    source.push_str("static ");
    push_c_type(source, &function.return_type);
    source.push(' ');
    push_c_function_name(source, &function.name);
    source.push_str("(void* gust_raw_env");

    for param in &function.params {
        source.push_str(", ");
        push_c_type(source, &param.type_);
        source.push(' ');
        push_c_local_name(source, &param.name);
    }

    source.push(')');
}

fn push_c_trait_dispatch_helpers(source: &mut String, program: &LoweredProgram) {
    if program.traits.is_empty() {
        return;
    }

    for trait_ in &program.traits {
        for impl_ in &trait_.impls {
            for method in &impl_.methods {
                let Some(function) = program
                    .functions
                    .iter()
                    .find(|function| function.name == method.function_name)
                else {
                    continue;
                };
                push_c_function_signature(source, function);
                source.push_str(";\n");
            }
        }
    }
    source.push('\n');

    for trait_ in &program.traits {
        for impl_ in &trait_.impls {
            let type_name = impl_.self_type.name();
            for method in &trait_.methods {
                let Some(impl_method) = impl_
                    .methods
                    .iter()
                    .find(|impl_method| impl_method.name == method.name)
                else {
                    continue;
                };

                source.push_str("static ");
                push_c_type(source, &method.return_type);
                source.push(' ');
                push_c_trait_thunk_name(source, &trait_.name, &type_name, &method.name);
                source.push_str("(void* gust_self");
                for (index, param) in method.params.iter().enumerate() {
                    source.push_str(", ");
                    push_c_type(source, &param.type_);
                    source.push(' ');
                    source.push_str("gust_arg");
                    source.push_str(&index.to_string());
                }
                source.push_str(") {\n");
                source.push_str("    ");
                if method.return_type != LoweredType::Void {
                    source.push_str("return ");
                }
                push_c_function_name(source, &impl_method.function_name);
                source.push('(');
                match &impl_.self_type {
                    LoweredType::Struct(struct_name) => {
                        source.push('(');
                        push_c_struct_name(source, struct_name);
                        source.push_str("*)gust_self");
                    }
                    LoweredType::Enum(enum_name) => {
                        source.push_str("*((");
                        push_c_enum_name(source, enum_name);
                        source.push_str("*)gust_self)");
                    }
                    _ => unreachable!("only struct and enum trait impls use dynamic dispatch"),
                }
                for index in 0..method.params.len() {
                    source.push_str(", gust_arg");
                    source.push_str(&index.to_string());
                }
                source.push_str(");\n");
                source.push_str("}\n\n");
            }

            source.push_str("static const ");
            push_c_trait_vtable_name(source, &trait_.name);
            source.push(' ');
            push_c_trait_impl_vtable_name(source, &trait_.name, &type_name);
            source.push_str(" = {\n");
            for method in &trait_.methods {
                source.push_str("    .");
                push_c_trait_method_field_name(source, &method.name);
                source.push_str(" = ");
                push_c_trait_thunk_name(source, &trait_.name, &type_name, &method.name);
                source.push_str(",\n");
            }
            source.push_str("};\n\n");
        }
    }
}

fn push_c_statement(
    source: &mut String,
    statement: &LoweredStatement,
    indent: usize,
    structs: &[LoweredStruct],
) {
    match statement {
        LoweredStatement::Local { name, value } => {
            push_c_indent(source, indent);
            push_c_type(source, &value.type_);
            source.push(' ');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::LocalCell { name, value } => {
            push_c_indent(source, indent);
            push_c_type(source, &value.type_);
            source.push_str("* ");
            push_c_local_name(source, name);
            source.push_str(" = gust_rt_alloc(sizeof(");
            push_c_type(source, &value.type_);
            source.push_str("));\n");
            push_c_indent(source, indent);
            source.push('*');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::Assignment { target, value } => {
            push_c_indent(source, indent);
            push_c_value(source, target, structs);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::Println(value) => match &value.kind {
            LoweredExprKind::StringLiteral(value) => {
                push_c_indent(source, indent);
                source.push_str("gust_rt_io_println(\"");
                push_c_string_value(source, value);
                source.push_str("\");\n");
            }
            LoweredExprKind::Local(name) => {
                push_c_indent(source, indent);
                source.push_str("gust_rt_io_println(");
                push_c_local_name(source, name);
                source.push_str(");\n");
            }
            LoweredExprKind::StringConcat(_, _)
            | LoweredExprKind::LocalCell(_)
            | LoweredExprKind::CapturedLocal { .. }
            | LoweredExprKind::Not(_)
            | LoweredExprKind::Logical { .. }
            | LoweredExprKind::Comparison { .. }
            | LoweredExprKind::FieldAccess { .. }
            | LoweredExprKind::EnumPayload { .. }
            | LoweredExprKind::MatchValue(_)
            | LoweredExprKind::Match { .. }
            | LoweredExprKind::NumberToString(_)
            | LoweredExprKind::Call { .. }
            | LoweredExprKind::CollectionLiteral { .. }
            | LoweredExprKind::DynamicCall { .. }
            | LoweredExprKind::IndirectCall { .. } => {
                push_c_indent(source, indent);
                source.push_str("gust_rt_io_println(");
                push_c_value(source, value, structs);
                source.push_str(");\n");
            }
            LoweredExprKind::Void
            | LoweredExprKind::BoolLiteral(_)
            | LoweredExprKind::NumberLiteral(_)
            | LoweredExprKind::PostfixIncrement(_)
            | LoweredExprKind::Negate(_)
            | LoweredExprKind::Arithmetic { .. }
            | LoweredExprKind::StructLiteral { .. }
            | LoweredExprKind::TraitObject { .. }
            | LoweredExprKind::Clone(_)
            | LoweredExprKind::Closure { .. }
            | LoweredExprKind::EnumLiteral { .. } => {
                unreachable!("println only lowers String values")
            }
        },
        LoweredStatement::Expr(value) => {
            push_c_indent(source, indent);
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::Return(value) => {
            push_c_indent(source, indent);
            source.push_str("return");

            if let Some(value) = value {
                source.push(' ');
                push_c_value(source, value, structs);
            }

            source.push_str(";\n");
        }
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            push_c_indent(source, indent);
            source.push_str("if (");
            push_c_value(source, condition, structs);
            source.push_str(") {\n");

            for statement in then_branch {
                push_c_statement(source, statement, indent + 1, structs);
            }

            push_c_indent(source, indent);
            source.push('}');

            if let Some(else_branch) = else_branch {
                source.push_str(" else {\n");

                for statement in else_branch {
                    push_c_statement(source, statement, indent + 1, structs);
                }

                push_c_indent(source, indent);
                source.push('}');
            }

            source.push('\n');
        }
        LoweredStatement::While { condition, body } => {
            push_c_indent(source, indent);
            source.push_str("while (");
            push_c_value(source, condition, structs);
            source.push_str(") {\n");

            for statement in body {
                push_c_statement(source, statement, indent + 1, structs);
            }

            push_c_indent(source, indent);
            source.push_str("}\n");
        }
        LoweredStatement::Break => {
            push_c_indent(source, indent);
            source.push_str("break;\n");
        }
        LoweredStatement::Continue => {
            push_c_indent(source, indent);
            source.push_str("continue;\n");
        }
        LoweredStatement::Match {
            value,
            temp_name,
            branches,
        } => {
            push_c_indent(source, indent);
            source.push_str("{\n");
            push_c_indent(source, indent + 1);
            push_c_type(source, &value.type_);
            source.push(' ');
            source.push_str(temp_name);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");

            for (index, branch) in branches.iter().enumerate() {
                push_c_indent(source, indent + 1);
                if index + 1 < branches.len() {
                    if index > 0 {
                        source.push_str("else ");
                    }
                    source.push_str("if (");
                    push_c_match_condition(source, temp_name, &branch.pattern);
                    source.push_str(") {\n");
                } else {
                    if index > 0 {
                        source.push_str("else ");
                    }
                    source.push_str("{\n");
                }

                for statement in &branch.statements {
                    push_c_statement(source, statement, indent + 2, structs);
                }

                push_c_indent(source, indent + 1);
                source.push_str("}\n");
            }

            push_c_indent(source, indent);
            source.push_str("}\n");
        }
    }
}

fn push_c_match_condition(source: &mut String, temp_name: &str, pattern: &LoweredPattern) {
    match pattern {
        LoweredPattern::Variant { enum_name, variant } => {
            source.push_str(temp_name);
            source.push_str(".gust_tag == ");
            push_c_enum_variant_tag(source, enum_name, variant);
        }
        LoweredPattern::String(value) => {
            source.push_str("gust_rt_string_equal(");
            source.push_str(temp_name);
            source.push_str(", \"");
            push_c_string_value(source, value);
            source.push_str("\")");
        }
        LoweredPattern::Wildcard => source.push_str("true"),
    }
}

fn push_c_indent(source: &mut String, indent: usize) {
    for _ in 0..indent {
        source.push_str("    ");
    }
}

fn push_c_local_name(source: &mut String, name: &str) {
    source.push_str("gust_");
    source.push_str(name);
}

fn push_c_function_name(source: &mut String, name: &str) {
    source.push_str("gust_fn_");
    source.push_str(&format!("{:08x}_", stable_name_hash(name)));
    push_c_identifier_suffix(source, name);
}

fn push_c_struct_name(source: &mut String, name: &str) {
    source.push_str("gust_struct_");
    source.push_str(&format!("{:08x}_", stable_name_hash(name)));
    push_c_identifier_suffix(source, name);
}

fn push_c_struct_new_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_new_");
    push_c_struct_name(source, name);
}

fn push_c_struct_clone_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_clone_");
    push_c_struct_name(source, name);
}

fn push_c_struct_clone_internal_name(source: &mut String, name: &str) {
    push_c_struct_clone_name(source, name);
    source.push_str("_internal");
}

fn push_c_enum_name(source: &mut String, name: &str) {
    source.push_str("gust_enum_");
    source.push_str(&format!("{:08x}_", stable_name_hash(name)));
    push_c_identifier_suffix(source, name);
}

fn push_c_trait_name(source: &mut String, name: &str) {
    source.push_str("gust_trait_");
    source.push_str(&format!("{:08x}_", stable_name_hash(name)));
    push_c_identifier_suffix(source, name);
}

fn push_c_trait_vtable_name(source: &mut String, name: &str) {
    push_c_trait_name(source, name);
    source.push_str("_vtable");
}

fn push_c_trait_method_field_name(source: &mut String, name: &str) {
    source.push_str("gust_method_");
    push_c_identifier_suffix(source, name);
}

fn push_c_trait_impl_vtable_name(source: &mut String, trait_name: &str, type_name: &str) {
    source.push_str("gust_vtable_");
    source.push_str(&format!(
        "{:08x}_",
        stable_name_hash(&format!("{trait_name} for {type_name}"))
    ));
    push_c_identifier_suffix(source, trait_name);
    source.push_str("_for_");
    push_c_identifier_suffix(source, type_name);
}

fn push_c_trait_thunk_name(
    source: &mut String,
    trait_name: &str,
    type_name: &str,
    method_name: &str,
) {
    source.push_str("gust_trait_thunk_");
    source.push_str(&format!(
        "{:08x}_",
        stable_name_hash(&format!("{trait_name} for {type_name}.{method_name}"))
    ));
    push_c_identifier_suffix(source, trait_name);
    source.push_str("_");
    push_c_identifier_suffix(source, type_name);
    source.push_str("_");
    push_c_identifier_suffix(source, method_name);
}

fn push_c_enum_clone_internal_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_clone_");
    push_c_enum_name(source, name);
    source.push_str("_internal");
}

fn push_c_enum_tag_name(source: &mut String, name: &str) {
    push_c_enum_name(source, name);
    source.push_str("_tag");
}

fn push_c_enum_variant_tag(source: &mut String, enum_name: &str, variant: &str) {
    push_c_enum_tag_name(source, enum_name);
    source.push('_');
    push_c_identifier_suffix(source, variant);
}

fn push_c_function_type_name(source: &mut String, type_: &LoweredType) {
    source.push_str("gust_fn_type_");
    source.push_str(&format!("{:08x}", stable_name_hash(&type_name_key(type_))));
}

fn closure_env_type_name(name: &str) -> String {
    format!(
        "gust_env_{:08x}_{}",
        stable_name_hash(name),
        sanitized_name(name)
    )
}

fn sanitized_name(name: &str) -> String {
    let mut result = String::new();
    push_c_identifier_suffix(&mut result, name);
    result
}

fn type_name_key(type_: &LoweredType) -> String {
    match type_ {
        LoweredType::Basic(type_) => type_.name().to_string(),
        LoweredType::Struct(name) | LoweredType::Enum(name) | LoweredType::Trait(name) => {
            name.clone()
        }
        LoweredType::Function {
            params,
            return_type,
        } => {
            let params = params
                .iter()
                .map(|param| {
                    if param.mutable {
                        format!("mut {}", type_name_key(&param.type_))
                    } else {
                        type_name_key(&param.type_)
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("fn({params}):{}", type_name_key(return_type))
        }
        LoweredType::Void => "void".to_string(),
    }
}

fn stable_name_hash(name: &str) -> u32 {
    let mut hash = 0x811c9dc5_u32;

    for byte in name.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x01000193);
    }

    hash
}

fn push_c_identifier_suffix(source: &mut String, name: &str) {
    for byte in name.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => source.push(byte as char),
            _ => source.push('_'),
        }
    }
}

fn push_c_type(source: &mut String, type_: &LoweredType) {
    match type_ {
        LoweredType::Basic(type_) => source.push_str(c_basic_type(*type_)),
        LoweredType::Struct(name) => {
            push_c_struct_name(source, name);
            source.push('*');
        }
        LoweredType::Enum(name) => push_c_enum_name(source, name),
        LoweredType::Trait(name) => push_c_trait_name(source, name),
        LoweredType::Function { .. } => push_c_function_type_name(source, type_),
        LoweredType::Void => source.push_str("void"),
    }
}

fn c_basic_type(type_: BasicType) -> &'static str {
    match type_ {
        BasicType::String => "const char*",
        BasicType::Bool => "bool",
        BasicType::U8 => "uint8_t",
        BasicType::U16 => "uint16_t",
        BasicType::U32 => "uint32_t",
        BasicType::U64 => "uint64_t",
        BasicType::U128 => "unsigned __int128",
        BasicType::Usize => "size_t",
        BasicType::I8 => "int8_t",
        BasicType::I16 => "int16_t",
        BasicType::I32 => "int32_t",
        BasicType::I64 => "int64_t",
        BasicType::I128 => "__int128",
        BasicType::F32 => "float",
        BasicType::F64 => "double",
    }
}

fn push_c_number_to_string_helper(source: &mut String, type_: BasicType) {
    if type_ == BasicType::U128 {
        source.push_str("static const char* gust_rt_u128_to_string(unsigned __int128 value) {\n");
        source.push_str("    char buffer[40];\n");
        source.push_str("    char* cursor = buffer + sizeof(buffer);\n");
        source.push_str("    *--cursor = '\\0';\n");
        source.push_str("    do {\n");
        source.push_str("        *--cursor = (char)('0' + value % 10);\n");
        source.push_str("        value /= 10;\n");
        source.push_str("    } while (value != 0);\n");
        source.push_str("    size_t length = (buffer + sizeof(buffer) - 1) - cursor;\n");
        source.push_str("    char* result = gust_rt_alloc(length + 1);\n");
        source.push_str("    memcpy(result, cursor, length + 1);\n");
        source.push_str("    return result;\n");
        source.push_str("}\n\n");
        return;
    }

    if type_ == BasicType::I128 {
        source.push_str("static const char* gust_rt_i128_to_string(__int128 value) {\n");
        source.push_str("    bool negative = value < 0;\n");
        source.push_str("    unsigned __int128 magnitude = negative\n");
        source.push_str("        ? (unsigned __int128)(-(value + 1)) + 1\n");
        source.push_str("        : (unsigned __int128)value;\n");
        source.push_str("    char buffer[41];\n");
        source.push_str("    char* cursor = buffer + sizeof(buffer);\n");
        source.push_str("    *--cursor = '\\0';\n");
        source.push_str("    do {\n");
        source.push_str("        *--cursor = (char)('0' + magnitude % 10);\n");
        source.push_str("        magnitude /= 10;\n");
        source.push_str("    } while (magnitude != 0);\n");
        source.push_str("    if (negative) {\n");
        source.push_str("        *--cursor = '-';\n");
        source.push_str("    }\n");
        source.push_str("    size_t length = (buffer + sizeof(buffer) - 1) - cursor;\n");
        source.push_str("    char* result = gust_rt_alloc(length + 1);\n");
        source.push_str("    memcpy(result, cursor, length + 1);\n");
        source.push_str("    return result;\n");
        source.push_str("}\n\n");
        return;
    }

    let (format, cast) = match type_ {
        BasicType::U8 | BasicType::U16 | BasicType::U32 => ("%u", "(unsigned int)value"),
        BasicType::U64 => ("%llu", "(unsigned long long)value"),
        BasicType::Usize => ("%zu", "value"),
        BasicType::I8 | BasicType::I16 | BasicType::I32 => ("%d", "(int)value"),
        BasicType::I64 => ("%lld", "(long long)value"),
        BasicType::F32 => ("%.9g", "(double)value"),
        BasicType::F64 => ("%.17g", "value"),
        BasicType::String | BasicType::Bool | BasicType::U128 | BasicType::I128 => {
            unreachable!("only directly formatted numeric types reach this path")
        }
    };

    source.push_str("static const char* gust_rt_");
    source.push_str(type_.name());
    source.push_str("_to_string(");
    source.push_str(c_basic_type(type_));
    source.push_str(" value) {\n");
    source.push_str("    int length = snprintf(NULL, 0, \"");
    source.push_str(format);
    source.push_str("\", ");
    source.push_str(cast);
    source.push_str(");\n");
    source.push_str("    char* result = gust_rt_alloc((size_t)length + 1);\n");
    source.push_str("    snprintf(result, (size_t)length + 1, \"");
    source.push_str(format);
    source.push_str("\", ");
    source.push_str(cast);
    source.push_str(");\n");
    source.push_str("    return result;\n");
    source.push_str("}\n\n");
}

fn push_c_number_literal(source: &mut String, value: &str, type_: &LoweredType) {
    match type_ {
        LoweredType::Basic(BasicType::F32) => {
            source.push_str(value);
            if !value.contains(['.', 'e', 'E']) {
                source.push_str(".0");
            }
            source.push('f');
        }
        LoweredType::Basic(BasicType::F64) => {
            source.push_str(value);
            if !value.contains(['.', 'e', 'E']) {
                source.push_str(".0");
            }
        }
        LoweredType::Basic(BasicType::U128) => push_c_u128_literal(source, value),
        LoweredType::Basic(BasicType::I128) => {
            source.push_str("((__int128)");
            push_c_u128_literal(source, value);
            source.push(')');
        }
        _ => source.push_str(value),
    }
}

fn push_c_u128_literal(source: &mut String, value: &str) {
    const CHUNK_DIGITS: usize = 18;
    const CHUNK_BASE: &str = "1000000000000000000ULL";

    let first_chunk_len = match value.len() % CHUNK_DIGITS {
        0 => CHUNK_DIGITS,
        len => len,
    };
    let remaining_chunks = (value.len() - first_chunk_len) / CHUNK_DIGITS;
    for _ in 0..remaining_chunks {
        source.push('(');
    }
    source.push_str("((unsigned __int128)");
    source.push_str(&value[..first_chunk_len]);
    source.push_str("ULL)");

    for chunk in value[first_chunk_len..].as_bytes().chunks(CHUNK_DIGITS) {
        source.push_str(" * (unsigned __int128)");
        source.push_str(CHUNK_BASE);
        source.push_str(" + ");
        source.push_str(std::str::from_utf8(chunk).expect("numeric literals are ASCII"));
        source.push_str("ULL)");
    }
}

fn push_c_value(source: &mut String, value: &LoweredExpr, structs: &[LoweredStruct]) {
    match &value.kind {
        LoweredExprKind::Void => {}
        LoweredExprKind::StringLiteral(value) => {
            source.push('"');
            push_c_string_value(source, value);
            source.push('"');
        }
        LoweredExprKind::BoolLiteral(value) => {
            if *value {
                source.push_str("true");
            } else {
                source.push_str("false");
            }
        }
        LoweredExprKind::NumberLiteral(literal) => {
            push_c_number_literal(source, literal, &value.type_)
        }
        LoweredExprKind::Local(name) => push_c_local_name(source, name),
        LoweredExprKind::LocalCell(name) => {
            source.push_str("(*");
            push_c_local_name(source, name);
            source.push(')');
        }
        LoweredExprKind::CapturedLocal { env_name, name } => {
            source.push_str("(*");
            push_c_local_name(source, env_name);
            source.push_str("->");
            push_c_local_name(source, name);
            source.push(')');
        }
        LoweredExprKind::PostfixIncrement(target) => {
            source.push('(');
            push_c_value(source, target, structs);
            source.push_str("++)");
        }
        LoweredExprKind::StringConcat(left, right) => {
            source.push_str("gust_rt_string_concat(");
            push_c_value(source, left, structs);
            source.push_str(", ");
            push_c_value(source, right, structs);
            source.push(')');
        }
        LoweredExprKind::Not(operand) => {
            source.push_str("(!");
            push_c_value(source, operand, structs);
            source.push(')');
        }
        LoweredExprKind::Negate(operand) => {
            if let LoweredExpr {
                type_: LoweredType::Basic(BasicType::I128),
                kind: LoweredExprKind::NumberLiteral(literal),
            } = operand.as_ref()
            {
                source.push_str("((__int128)(-");
                push_c_u128_literal(source, literal);
                source.push_str("))");
                return;
            }

            source.push_str("(-");
            push_c_value(source, operand, structs);
            source.push(')');
        }
        LoweredExprKind::Arithmetic { left, op, right } => {
            if *op == BinaryOp::Remainder
                && matches!(
                    left.type_,
                    LoweredType::Basic(BasicType::F32 | BasicType::F64)
                )
            {
                if left.type_ == LoweredType::Basic(BasicType::F32) {
                    source.push_str("fmodf(");
                } else {
                    source.push_str("fmod(");
                }
                push_c_value(source, left, structs);
                source.push_str(", ");
                push_c_value(source, right, structs);
                source.push(')');
                return;
            }

            source.push('(');
            push_c_value(source, left, structs);
            source.push(' ');
            source.push_str(op.symbol());
            source.push(' ');
            push_c_value(source, right, structs);
            source.push(')');
        }
        LoweredExprKind::Logical { left, op, right } => {
            source.push('(');
            push_c_value(source, left, structs);
            source.push(' ');
            source.push_str(op.symbol());
            source.push(' ');
            push_c_value(source, right, structs);
            source.push(')');
        }
        LoweredExprKind::Comparison { left, op, right } => {
            if left.type_ == LoweredType::Basic(BasicType::String) {
                if *op == BinaryOp::NotEqual {
                    source.push('!');
                }

                source.push_str("gust_rt_string_equal(");
                push_c_value(source, left, structs);
                source.push_str(", ");
                push_c_value(source, right, structs);
                source.push(')');
            } else {
                source.push('(');
                push_c_value(source, left, structs);
                source.push(' ');
                source.push_str(op.symbol());
                source.push(' ');
                push_c_value(source, right, structs);
                source.push(')');
            }
        }
        LoweredExprKind::StructLiteral { name, fields } => {
            push_c_struct_new_name(source, name);
            source.push('(');
            let struct_ = structs
                .iter()
                .find(|struct_| struct_.name == *name)
                .expect("lowered struct literal must reference a known struct");

            for (index, field) in struct_.fields.iter().enumerate() {
                if index > 0 {
                    source.push_str(", ");
                }

                let value = fields
                    .iter()
                    .find(|value| value.name == field.name)
                    .expect("lowered struct literal must contain every declared field");
                push_c_value(source, &value.value, structs);
            }

            source.push(')');
        }
        LoweredExprKind::EnumLiteral {
            enum_name,
            variant,
            payload,
        } => {
            source.push('(');
            push_c_enum_name(source, enum_name);
            source.push_str("){ .gust_tag = ");
            push_c_enum_variant_tag(source, enum_name, variant);

            if let Some(payload) = payload {
                source.push_str(", .gust_payload.");
                push_c_local_name(source, variant);
                source.push_str(" = ");
                push_c_value(source, payload, structs);
            }

            source.push_str(" }");
        }
        LoweredExprKind::EnumPayload { object, variant } => {
            push_c_value(source, object, structs);
            source.push_str(".gust_payload.");
            push_c_local_name(source, variant);
        }
        LoweredExprKind::MatchValue(name) => source.push_str(name),
        LoweredExprKind::Match {
            value: matched_value,
            temp_name,
            branches,
        } => {
            let result_name = format!("{temp_name}_result");

            source.push_str("({\n    ");
            push_c_type(source, &matched_value.type_);
            source.push(' ');
            source.push_str(temp_name);
            source.push_str(" = ");
            push_c_value(source, matched_value, structs);
            source.push_str(";\n    ");
            push_c_type(source, &value.type_);
            source.push(' ');
            source.push_str(&result_name);
            source.push_str(";\n");

            for (index, branch) in branches.iter().enumerate() {
                source.push_str("    ");
                if index + 1 < branches.len() {
                    if index > 0 {
                        source.push_str("else ");
                    }
                    source.push_str("if (");
                    push_c_match_condition(source, temp_name, &branch.pattern);
                    source.push_str(") {\n");
                } else {
                    if index > 0 {
                        source.push_str("else ");
                    }
                    source.push_str("{\n");
                }

                for statement in &branch.statements {
                    push_c_statement(source, statement, 2, structs);
                }

                push_c_indent(source, 2);
                source.push_str(&result_name);
                source.push_str(" = ");
                push_c_value(source, &branch.value, structs);
                source.push_str(";\n");

                source.push_str("    }\n");
            }

            source.push_str("    ");
            source.push_str(&result_name);
            source.push_str(";\n})");
        }
        LoweredExprKind::FieldAccess { object, field } => {
            push_c_value(source, object, structs);
            source.push_str("->");
            push_c_local_name(source, field);
        }
        LoweredExprKind::Clone(object) => {
            let LoweredType::Struct(name) = &object.type_ else {
                unreachable!("only struct values use lowered clone expressions")
            };
            push_c_struct_clone_name(source, name);
            source.push('(');
            push_c_value(source, object, structs);
            source.push(')');
        }
        LoweredExprKind::NumberToString(object) => {
            let LoweredType::Basic(type_) = &object.type_ else {
                unreachable!("only basic numeric values use number-to-string expressions")
            };
            source.push_str("gust_rt_");
            source.push_str(type_.name());
            source.push_str("_to_string(");
            push_c_value(source, object, structs);
            source.push(')');
        }
        LoweredExprKind::Call { name, args } => {
            let raw_element = raw_buffer_element_type(structs, &value.type_).or_else(|| {
                args.first()
                    .and_then(|arg| raw_buffer_element_type(structs, &arg.type_))
            });
            if let (Some(method), Some(element)) = (raw_buffer_method(name), raw_element) {
                match method {
                    "withCapacity" => {
                        source.push_str("({\n    ");
                        push_c_type(source, &value.type_);
                        source.push_str(" gust_buffer = gust_rt_alloc(sizeof(*gust_buffer));\n");
                        source.push_str("    memset(gust_buffer, 0, sizeof(*gust_buffer));\n");
                        source.push_str("    gust_buffer->gust_capacity = ");
                        push_c_value(source, &args[0], structs);
                        source.push_str(";\n    if (gust_buffer->gust_capacity > 0) {\n        gust_buffer->gust_data = gust_rt_alloc(sizeof(");
                        push_c_type(source, element);
                        source.push_str(
                            ") * gust_buffer->gust_capacity);\n    }\n    gust_buffer;\n})",
                        );
                        return;
                    }
                    "capacity" => {
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_capacity");
                        return;
                    }
                    "read" => {
                        let LoweredType::Enum(option) = &value.type_ else {
                            unreachable!("raw buffer reads return Option values")
                        };
                        source.push('(');
                        push_c_enum_name(source, option);
                        source.push_str("){ .gust_tag = ");
                        push_c_enum_variant_tag(source, option, "Some");
                        source.push_str(", .gust_payload.");
                        push_c_local_name(source, "Some");
                        source.push_str(" = ((");
                        push_c_type(source, element);
                        source.push_str("*)");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_data)[");
                        push_c_value(source, &args[1], structs);
                        source.push_str("] }");
                        return;
                    }
                    "write" => {
                        source.push_str("((");
                        push_c_type(source, element);
                        source.push_str("*)");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_data)[");
                        push_c_value(source, &args[1], structs);
                        source.push_str("] = ");
                        push_c_value(source, &args[2], structs);
                        source.push_str(", ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length = ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length > ");
                        push_c_value(source, &args[1], structs);
                        source.push_str(" ? ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length : ");
                        push_c_value(source, &args[1], structs);
                        source.push_str(" + 1");
                        return;
                    }
                    "clear" => {
                        source.push_str("({ memset(&( (");
                        push_c_type(source, element);
                        source.push_str("*)");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_data)[");
                        push_c_value(source, &args[1], structs);
                        source.push_str("], 0, sizeof(");
                        push_c_type(source, element);
                        source.push_str(")); if (");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length == ");
                        push_c_value(source, &args[1], structs);
                        source.push_str(" + 1) { ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length = ");
                        push_c_value(source, &args[1], structs);
                        source.push_str("; } })");
                        return;
                    }
                    "grow" => {
                        source.push_str("({ ");
                        push_c_type(source, &args[0].type_);
                        source.push_str(" gust_buffer = ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("; size_t gust_capacity = ");
                        push_c_value(source, &args[1], structs);
                        source.push_str("; void* gust_data = gust_rt_alloc(sizeof(");
                        push_c_type(source, element);
                        source.push_str(") * gust_capacity); if (gust_buffer->gust_length > 0) { memcpy(gust_data, gust_buffer->gust_data, sizeof(");
                        push_c_type(source, element);
                        source.push_str(") * gust_buffer->gust_length); } gust_buffer->gust_data = gust_data; gust_buffer->gust_capacity = gust_capacity; })");
                        return;
                    }
                    _ => unreachable!("raw buffer methods are exhaustive"),
                }
            }
            push_c_function_name(source, name);
            source.push('(');

            for (index, arg) in args.iter().enumerate() {
                if index > 0 {
                    source.push_str(", ");
                }

                push_c_value(source, arg, structs);
            }

            source.push(')');
        }
        LoweredExprKind::CollectionLiteral {
            constructor,
            add,
            items,
        } => {
            source.push_str("({\n    ");
            push_c_type(source, &value.type_);
            source.push_str(" gust_collection = ");
            push_c_function_name(source, constructor);
            source.push('(');
            source.push_str(&items.len().to_string());
            source.push_str(");\n");
            for item in items {
                source.push_str("    ");
                push_c_function_name(source, add);
                source.push_str("(gust_collection, ");
                push_c_value(source, item, structs);
                source.push_str(");\n");
            }
            source.push_str("    gust_collection;\n})");
        }
        LoweredExprKind::TraitObject {
            trait_name,
            self_type,
            value,
        } => match self_type {
            LoweredType::Struct(type_name) => {
                source.push('(');
                push_c_trait_name(source, trait_name);
                source.push_str("){ .gust_self = ");
                push_c_value(source, value, structs);
                source.push_str(", .gust_vtable = &");
                push_c_trait_impl_vtable_name(source, trait_name, type_name);
                source.push_str(" }");
            }
            LoweredType::Enum(type_name) => {
                source.push_str("({\n    ");
                push_c_enum_name(source, type_name);
                source.push_str("* gust_trait_self = gust_rt_alloc(sizeof(");
                push_c_enum_name(source, type_name);
                source.push_str("));\n    *gust_trait_self = ");
                push_c_value(source, value, structs);
                source.push_str(";\n    (");
                push_c_trait_name(source, trait_name);
                source.push_str("){ .gust_self = gust_trait_self, .gust_vtable = &");
                push_c_trait_impl_vtable_name(source, trait_name, type_name);
                source.push_str(" };\n})");
            }
            _ => unreachable!("only struct and enum values can be emitted as trait objects"),
        },
        LoweredExprKind::DynamicCall {
            object,
            method,
            args,
        } => {
            let LoweredType::Trait(trait_name) = &object.type_ else {
                unreachable!("dynamic calls require trait-typed receivers")
            };
            source.push_str("({\n    ");
            push_c_trait_name(source, trait_name);
            source.push_str(" gust_trait_value = ");
            push_c_value(source, object, structs);
            source.push_str(";\n    ");
            if value.type_ != LoweredType::Void {
                push_c_type(source, &value.type_);
                source.push_str(" gust_trait_result = ");
            }
            source.push_str("gust_trait_value.gust_vtable->");
            push_c_trait_method_field_name(source, method);
            source.push_str("(gust_trait_value.gust_self");
            for arg in args {
                source.push_str(", ");
                push_c_value(source, arg, structs);
            }
            source.push_str(");\n");
            if value.type_ != LoweredType::Void {
                source.push_str("    gust_trait_result;\n");
            }
            source.push_str("})");
        }
        LoweredExprKind::Closure { name, captures } => {
            let LoweredType::Function { .. } = &value.type_ else {
                unreachable!("closure expressions must have function type")
            };
            if captures.is_empty() {
                source.push('(');
                push_c_type(source, &value.type_);
                source.push_str("){ .gust_env = NULL, .gust_call = ");
                push_c_function_name(source, name);
                source.push_str(" }");
            } else {
                let env_type = closure_env_type_name(name);
                source.push_str("({\n    ");
                source.push_str(&env_type);
                source.push_str("* gust_env = gust_rt_alloc(sizeof(");
                source.push_str(&env_type);
                source.push_str("));\n");
                for capture in captures {
                    source.push_str("    gust_env->");
                    push_c_local_name(source, &capture.name);
                    source.push_str(" = ");
                    push_c_local_name(source, &capture.name);
                    source.push_str(";\n");
                }
                source.push_str("    (");
                push_c_type(source, &value.type_);
                source.push_str("){ .gust_env = gust_env, .gust_call = ");
                push_c_function_name(source, name);
                source.push_str(" };\n})");
            }
        }
        LoweredExprKind::IndirectCall { callee, args } => {
            push_c_value(source, callee, structs);
            source.push_str(".gust_call(");
            push_c_value(source, callee, structs);
            source.push_str(".gust_env");
            for arg in args {
                source.push_str(", ");
                push_c_value(source, arg, structs);
            }
            source.push(')');
        }
    }
}

fn push_c_string_value(source: &mut String, value: &str) {
    for byte in value.bytes() {
        match byte {
            b'\n' => source.push_str("\\n"),
            b'\r' => source.push_str("\\r"),
            b'\t' => source.push_str("\\t"),
            b'"' => source.push_str("\\\""),
            b'\\' => source.push_str("\\\\"),
            b' '..=b'~' => source.push(byte as char),
            _ => source.push_str(&format!("\\{byte:03o}")),
        }
    }
}
