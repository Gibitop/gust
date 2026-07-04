use std::collections::HashSet;

use crate::ast::{BasicType, BinaryOp};
use crate::lower::{
    LoweredEnum, LoweredExpr, LoweredExprKind, LoweredFunction, LoweredProgram, LoweredStatement,
    LoweredStruct, LoweredType,
};

pub fn emit_c(program: &LoweredProgram) -> String {
    let uses_bool = program_uses_type(program, BasicType::Bool);
    let uses_usize = program_uses_type(program, BasicType::Usize);
    let uses_float =
        program_uses_type(program, BasicType::F32) || program_uses_type(program, BasicType::F64);
    let uses_fixed_width_int = program_uses_fixed_width_int(program);
    let uses_string_concat = program_uses_string_concat(program);
    let uses_string_equality = program_uses_string_equality(program);
    let uses_println = program.statements.iter().any(statement_uses_println)
        || program
            .functions
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

    if uses_println {
        source.push_str("#include <stdio.h>\n");
    }

    if uses_float {
        source.push_str("#include <math.h>\n");
    }

    let uses_alloc = uses_string_concat || !program.structs.is_empty();

    if uses_alloc {
        source.push_str("#include <stdlib.h>\n#include <string.h>\n");
    } else if uses_string_equality {
        source.push_str("#include <string.h>\n");
    }

    if !source.is_empty() {
        source.push('\n');
    }

    push_c_type_definitions(&mut source, program);

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

    for function in ordered_functions(&program.functions) {
        if function_calls_name(function, &function.name) {
            push_c_function_signature(&mut source, function);
            source.push_str(";\n\n");
        }

        push_c_function(&mut source, function);
        source.push('\n');
    }

    source.push_str("int main(void) {\n");

    for statement in &program.statements {
        push_c_statement(&mut source, statement, 1);
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

fn statement_uses_type(statement: &LoweredStatement, type_: BasicType) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
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
                    || branches
                        .iter()
                        .any(|branch| expr_uses_type(&branch.value, type_))
            }
            LoweredExprKind::FieldAccess { object, .. } | LoweredExprKind::Clone(object) => {
                expr_uses_type(object, type_)
            }
            LoweredExprKind::Call { args, .. } => args.iter().any(|arg| expr_uses_type(arg, type_)),
            LoweredExprKind::Void
            | LoweredExprKind::StringLiteral(_)
            | LoweredExprKind::BoolLiteral(_)
            | LoweredExprKind::NumberLiteral(_)
            | LoweredExprKind::Local(_) => false,
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

fn program_uses_string_concat(program: &LoweredProgram) -> bool {
    program
        .functions
        .iter()
        .any(|function| function_uses_string_concat(function))
        || program.statements.iter().any(statement_uses_string_concat)
}

fn program_uses_string_equality(program: &LoweredProgram) -> bool {
    program.functions.iter().any(|function| {
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
                || branches
                    .iter()
                    .any(|branch| expr_uses_string_equality(&branch.value))
        }
        LoweredExprKind::FieldAccess { object, .. } | LoweredExprKind::Clone(object) => {
            expr_uses_string_equality(object)
        }
        LoweredExprKind::Call { args, .. } => args.iter().any(expr_uses_string_equality),
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_) => false,
    }
}

fn function_uses_string_concat(function: &LoweredFunction) -> bool {
    function.statements.iter().any(statement_uses_string_concat)
        || expr_uses_string_concat(&function.return_value)
}

fn statement_uses_string_concat(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Local { value, .. }
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
                || branches
                    .iter()
                    .any(|branch| expr_uses_string_concat(&branch.value))
        }
        LoweredExprKind::FieldAccess { object, .. } | LoweredExprKind::Clone(object) => {
            expr_uses_string_concat(object)
        }
        LoweredExprKind::Call { args, .. } => args.iter().any(expr_uses_string_concat),
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_) => false,
    }
}

fn statement_uses_println(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Println(_) => true,
        LoweredStatement::If {
            then_branch,
            else_branch,
            ..
        } => {
            then_branch.iter().any(statement_uses_println)
                || else_branch
                    .as_ref()
                    .is_some_and(|statements| statements.iter().any(statement_uses_println))
        }
        LoweredStatement::Local { .. }
        | LoweredStatement::Assignment { .. }
        | LoweredStatement::Expr(_)
        | LoweredStatement::Return(_) => false,
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
                || branches
                    .iter()
                    .any(|branch| expr_calls_name(&branch.value, name))
        }
        LoweredExprKind::FieldAccess { object, .. } | LoweredExprKind::Clone(object) => {
            expr_calls_name(object, name)
        }
        LoweredExprKind::Call {
            name: called_name,
            args,
        } => called_name == name || args.iter().any(|arg| expr_calls_name(arg, name)),
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_) => false,
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

    source.push_str("};\n");
}

fn push_c_type_definitions(source: &mut String, program: &LoweredProgram) {
    let mut emitted = HashSet::new();
    let mut remaining = program.structs.len() + program.enums.len();

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
}

fn type_definition_is_emitted(type_: &LoweredType, emitted: &HashSet<String>) -> bool {
    match type_ {
        LoweredType::Basic(_) | LoweredType::Struct(_) | LoweredType::Void => true,
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
                Some(LoweredType::Basic(_)) | None => {}
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
        push_c_struct_name(source, &struct_.name);
        source.push_str(" value) {\n    ");
        push_c_struct_name(source, &struct_.name);
        source.push_str("* result = gust_rt_alloc(sizeof(");
        push_c_struct_name(source, &struct_.name);
        source.push_str("));\n");
        source.push_str("    *result = value;\n");
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

fn push_c_function(source: &mut String, function: &LoweredFunction) {
    source.push_str("// Gust function: ");
    source.push_str(&function.name);
    source.push('\n');
    push_c_function_signature(source, function);
    source.push_str(" {\n");

    for statement in &function.statements {
        push_c_statement(source, statement, 1);
    }

    if function.return_type != LoweredType::Void && function.return_value.type_ != LoweredType::Void
    {
        source.push_str("    return ");
        push_c_value(source, &function.return_value);
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

fn push_c_statement(source: &mut String, statement: &LoweredStatement, indent: usize) {
    match statement {
        LoweredStatement::Local { name, value } => {
            push_c_indent(source, indent);
            push_c_type(source, &value.type_);
            source.push(' ');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value(source, value);
            source.push_str(";\n");
        }
        LoweredStatement::Assignment { target, value } => {
            push_c_indent(source, indent);
            push_c_value(source, target);
            source.push_str(" = ");
            push_c_value(source, value);
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
            | LoweredExprKind::Not(_)
            | LoweredExprKind::Logical { .. }
            | LoweredExprKind::Comparison { .. }
            | LoweredExprKind::FieldAccess { .. }
            | LoweredExprKind::EnumPayload { .. }
            | LoweredExprKind::Match { .. }
            | LoweredExprKind::Call { .. } => {
                push_c_indent(source, indent);
                source.push_str("gust_rt_io_println(");
                push_c_value(source, value);
                source.push_str(");\n");
            }
            LoweredExprKind::Void
            | LoweredExprKind::BoolLiteral(_)
            | LoweredExprKind::NumberLiteral(_)
            | LoweredExprKind::PostfixIncrement(_)
            | LoweredExprKind::Negate(_)
            | LoweredExprKind::Arithmetic { .. }
            | LoweredExprKind::StructLiteral { .. }
            | LoweredExprKind::Clone(_)
            | LoweredExprKind::EnumLiteral { .. } => {
                unreachable!("println only lowers String values")
            }
        },
        LoweredStatement::Expr(value) => {
            push_c_indent(source, indent);
            push_c_value(source, value);
            source.push_str(";\n");
        }
        LoweredStatement::Return(value) => {
            push_c_indent(source, indent);
            source.push_str("return");

            if let Some(value) = value {
                source.push(' ');
                push_c_value(source, value);
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
            push_c_value(source, condition);
            source.push_str(") {\n");

            for statement in then_branch {
                push_c_statement(source, statement, indent + 1);
            }

            push_c_indent(source, indent);
            source.push('}');

            if let Some(else_branch) = else_branch {
                source.push_str(" else {\n");

                for statement in else_branch {
                    push_c_statement(source, statement, indent + 1);
                }

                push_c_indent(source, indent);
                source.push('}');
            }

            source.push('\n');
        }
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

fn push_c_value(source: &mut String, value: &LoweredExpr) {
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
        LoweredExprKind::PostfixIncrement(target) => {
            source.push('(');
            push_c_value(source, target);
            source.push_str("++)");
        }
        LoweredExprKind::StringConcat(left, right) => {
            source.push_str("gust_rt_string_concat(");
            push_c_value(source, left);
            source.push_str(", ");
            push_c_value(source, right);
            source.push(')');
        }
        LoweredExprKind::Not(operand) => {
            source.push_str("(!");
            push_c_value(source, operand);
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
            push_c_value(source, operand);
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
                push_c_value(source, left);
                source.push_str(", ");
                push_c_value(source, right);
                source.push(')');
                return;
            }

            source.push('(');
            push_c_value(source, left);
            source.push(' ');
            source.push_str(op.symbol());
            source.push(' ');
            push_c_value(source, right);
            source.push(')');
        }
        LoweredExprKind::Logical { left, op, right } => {
            source.push('(');
            push_c_value(source, left);
            source.push(' ');
            source.push_str(op.symbol());
            source.push(' ');
            push_c_value(source, right);
            source.push(')');
        }
        LoweredExprKind::Comparison { left, op, right } => {
            if left.type_ == LoweredType::Basic(BasicType::String) {
                if *op == BinaryOp::NotEqual {
                    source.push('!');
                }

                source.push_str("gust_rt_string_equal(");
                push_c_value(source, left);
                source.push_str(", ");
                push_c_value(source, right);
                source.push(')');
            } else {
                source.push('(');
                push_c_value(source, left);
                source.push(' ');
                source.push_str(op.symbol());
                source.push(' ');
                push_c_value(source, right);
                source.push(')');
            }
        }
        LoweredExprKind::StructLiteral { name, fields } => {
            push_c_struct_new_name(source, name);
            source.push_str("((");
            push_c_struct_name(source, name);
            source.push_str("){");

            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    source.push_str(", ");
                } else {
                    source.push(' ');
                }

                source.push('.');
                push_c_local_name(source, &field.name);
                source.push_str(" = ");
                push_c_value(source, &field.value);
            }

            if !fields.is_empty() {
                source.push(' ');
            }

            source.push_str("})");
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
                push_c_value(source, payload);
            }

            source.push_str(" }");
        }
        LoweredExprKind::EnumPayload { object, variant } => {
            push_c_value(source, object);
            source.push_str(".gust_payload.");
            push_c_local_name(source, variant);
        }
        LoweredExprKind::Match {
            value,
            enum_name,
            branches,
        } => {
            source.push('(');

            for (index, branch) in branches.iter().enumerate() {
                if index + 1 < branches.len() {
                    push_c_value(source, value);
                    source.push_str(".gust_tag == ");
                    push_c_enum_variant_tag(source, enum_name, &branch.variant);
                    source.push_str(" ? ");
                }

                push_c_value(source, &branch.value);

                if index + 1 < branches.len() {
                    source.push_str(" : ");
                }
            }

            source.push(')');
        }
        LoweredExprKind::FieldAccess { object, field } => {
            push_c_value(source, object);
            source.push_str("->");
            push_c_local_name(source, field);
        }
        LoweredExprKind::Clone(object) => {
            let LoweredType::Struct(name) = &object.type_ else {
                unreachable!("only struct values use lowered clone expressions")
            };
            push_c_struct_clone_name(source, name);
            source.push('(');
            push_c_value(source, object);
            source.push(')');
        }
        LoweredExprKind::Call { name, args } => {
            push_c_function_name(source, name);
            source.push('(');

            for (index, arg) in args.iter().enumerate() {
                if index > 0 {
                    source.push_str(", ");
                }

                push_c_value(source, arg);
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
