use crate::ast::BasicType;
use crate::lower::{
    LoweredExpr, LoweredExprKind, LoweredFunction, LoweredProgram, LoweredStatement,
};

pub fn emit_c(program: &LoweredProgram) -> String {
    let uses_bool = program_uses_type(program, BasicType::Bool);
    let uses_usize = program_uses_type(program, BasicType::Usize);
    let uses_fixed_width_int = program_uses_fixed_width_int(program);
    let uses_string_concat = program_uses_string_concat(program);
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

    if uses_string_concat {
        source.push_str("#include <stdlib.h>\n#include <string.h>\n");
    }

    if !source.is_empty() {
        source.push('\n');
    }

    if uses_string_concat {
        source.push_str("static void* gust_rt_alloc(size_t size) {\n");
        source.push_str("    return malloc(size);\n");
        source.push_str("}\n\n");
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

    if uses_println {
        source.push_str("static void gust_rt_io_println(const char* value) {\n");
        source.push_str("    puts(value);\n");
        source.push_str("}\n\n");
    }

    for function in &program.functions {
        push_c_function(&mut source, function);
        source.push('\n');
    }

    source.push_str("int main(void) {\n");

    for statement in &program.statements {
        push_c_statement(&mut source, statement);
    }

    source.push_str("    return 0;\n}\n");
    source
}

fn program_uses_type(program: &LoweredProgram, type_: BasicType) -> bool {
    program
        .functions
        .iter()
        .any(|function| function_uses_type(function, type_))
        || program
            .statements
            .iter()
            .any(|statement| statement_uses_type(statement, type_))
}

fn function_uses_type(function: &LoweredFunction, type_: BasicType) -> bool {
    function.return_type == type_
        || function.params.iter().any(|param| param.type_ == type_)
        || function
            .statements
            .iter()
            .any(|statement| statement_uses_type(statement, type_))
        || expr_uses_type(&function.return_value, type_)
}

fn statement_uses_type(statement: &LoweredStatement, type_: BasicType) -> bool {
    match statement {
        LoweredStatement::Local { value, .. } | LoweredStatement::Println(value) => {
            expr_uses_type(value, type_)
        }
    }
}

fn expr_uses_type(expr: &LoweredExpr, type_: BasicType) -> bool {
    expr.type_ == type_
        || match &expr.kind {
            LoweredExprKind::StringConcat(left, right) => {
                expr_uses_type(left, type_) || expr_uses_type(right, type_)
            }
            LoweredExprKind::Call { args, .. } => args.iter().any(|arg| expr_uses_type(arg, type_)),
            LoweredExprKind::StringLiteral(_)
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

fn function_uses_string_concat(function: &LoweredFunction) -> bool {
    function.statements.iter().any(statement_uses_string_concat)
        || expr_uses_string_concat(&function.return_value)
}

fn statement_uses_string_concat(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Local { value, .. } | LoweredStatement::Println(value) => {
            expr_uses_string_concat(value)
        }
    }
}

fn expr_uses_string_concat(expr: &LoweredExpr) -> bool {
    match &expr.kind {
        LoweredExprKind::StringConcat(_, _) => true,
        LoweredExprKind::Call { args, .. } => args.iter().any(expr_uses_string_concat),
        LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_) => false,
    }
}

fn statement_uses_println(statement: &LoweredStatement) -> bool {
    matches!(statement, LoweredStatement::Println(_))
}

fn push_c_function(source: &mut String, function: &LoweredFunction) {
    source.push_str("// Gust function: ");
    source.push_str(&function.name);
    source.push('\n');
    source.push_str("static ");
    source.push_str(c_type(function.return_type));
    source.push(' ');
    push_c_function_name(source, &function.name);
    source.push('(');

    for (index, param) in function.params.iter().enumerate() {
        if index > 0 {
            source.push_str(", ");
        }

        source.push_str(c_type(param.type_));
        source.push(' ');
        push_c_local_name(source, &param.name);
    }

    source.push_str(") {\n");

    for statement in &function.statements {
        push_c_statement(source, statement);
    }

    source.push_str("    return ");
    push_c_value(source, &function.return_value);
    source.push_str(";\n");
    source.push_str("}\n");
}

fn push_c_statement(source: &mut String, statement: &LoweredStatement) {
    match statement {
        LoweredStatement::Local { name, value } => {
            source.push_str("    ");
            source.push_str(c_type(value.type_));
            source.push(' ');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value(source, value);
            source.push_str(";\n");
        }
        LoweredStatement::Println(value) => match &value.kind {
            LoweredExprKind::StringLiteral(value) => {
                source.push_str("    gust_rt_io_println(\"");
                push_c_string_value(source, value);
                source.push_str("\");\n");
            }
            LoweredExprKind::Local(name) => {
                source.push_str("    gust_rt_io_println(");
                push_c_local_name(source, name);
                source.push_str(");\n");
            }
            LoweredExprKind::StringConcat(_, _) | LoweredExprKind::Call { .. } => {
                source.push_str("    gust_rt_io_println(");
                push_c_value(source, value);
                source.push_str(");\n");
            }
            LoweredExprKind::BoolLiteral(_) | LoweredExprKind::NumberLiteral(_) => {
                unreachable!("println only lowers String values")
            }
        },
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

fn c_type(type_: BasicType) -> &'static str {
    match type_ {
        BasicType::String => "const char*",
        BasicType::Bool => "bool",
        BasicType::U8 => "uint8_t",
        BasicType::U16 => "uint16_t",
        BasicType::U32 => "uint32_t",
        BasicType::U64 => "uint64_t",
        BasicType::Usize => "size_t",
        BasicType::I8 => "int8_t",
        BasicType::I16 => "int16_t",
        BasicType::I32 => "int32_t",
        BasicType::I64 => "int64_t",
    }
}

fn push_c_value(source: &mut String, value: &LoweredExpr) {
    match &value.kind {
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
        LoweredExprKind::NumberLiteral(value) => source.push_str(value),
        LoweredExprKind::Local(name) => push_c_local_name(source, name),
        LoweredExprKind::StringConcat(left, right) => {
            source.push_str("gust_rt_string_concat(");
            push_c_value(source, left);
            source.push_str(", ");
            push_c_value(source, right);
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
