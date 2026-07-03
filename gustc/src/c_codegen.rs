use crate::ast::BasicType;
use crate::lower::{LoweredExpr, LoweredExprKind, LoweredProgram, LoweredStatement};

pub fn emit_c(program: &LoweredProgram) -> String {
    let uses_bool = program.statements.iter().any(|statement| match statement {
        LoweredStatement::Local { value, .. } => value.type_ == BasicType::Bool,
        LoweredStatement::Println(_) => false,
    });
    let uses_usize = program.statements.iter().any(|statement| match statement {
        LoweredStatement::Local { value, .. } => value.type_ == BasicType::Usize,
        LoweredStatement::Println(_) => false,
    });
    let uses_fixed_width_int = program.statements.iter().any(|statement| match statement {
        LoweredStatement::Local { value, .. } => matches!(
            value.type_,
            BasicType::U8
                | BasicType::U16
                | BasicType::U32
                | BasicType::U64
                | BasicType::I8
                | BasicType::I16
                | BasicType::I32
                | BasicType::I64
        ),
        LoweredStatement::Println(_) => false,
    });
    let uses_string_concat = program.statements.iter().any(|statement| match statement {
        LoweredStatement::Local { value, .. } | LoweredStatement::Println(value) => {
            matches!(&value.kind, LoweredExprKind::StringConcat(_, _))
        }
    });
    let uses_println = program
        .statements
        .iter()
        .any(|statement| matches!(statement, LoweredStatement::Println(_)));

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
        source.push_str("static void* gust_alloc(size_t size) {\n");
        source.push_str("    return malloc(size);\n");
        source.push_str("}\n\n");
        source.push_str("static char* gust_string_concat(const char* left, const char* right) {\n");
        source.push_str("    size_t left_len = strlen(left);\n");
        source.push_str("    size_t right_len = strlen(right);\n");
        source.push_str("    char* result = gust_alloc(left_len + right_len + 1);\n");
        source.push_str("    memcpy(result, left, left_len);\n");
        source.push_str("    memcpy(result + left_len, right, right_len + 1);\n");
        source.push_str("    return result;\n");
        source.push_str("}\n\n");
    }

    if uses_println {
        source.push_str("static void gust_io_println(const char* value) {\n");
        source.push_str("    puts(value);\n");
        source.push_str("}\n\n");
    }

    source.push_str("int main(void) {\n");

    for statement in &program.statements {
        match statement {
            LoweredStatement::Local { name, value } => {
                source.push_str("    ");
                source.push_str(c_type(value.type_));
                source.push(' ');
                push_c_local_name(&mut source, name);
                source.push_str(" = ");
                push_c_value(&mut source, value);
                source.push_str(";\n");
            }
            LoweredStatement::Println(value) => match &value.kind {
                LoweredExprKind::StringLiteral(value) => {
                    source.push_str("    gust_io_println(\"");
                    push_c_string_value(&mut source, value);
                    source.push_str("\");\n");
                }
                LoweredExprKind::Local(name) => {
                    source.push_str("    gust_io_println(");
                    push_c_local_name(&mut source, name);
                    source.push_str(");\n");
                }
                LoweredExprKind::StringConcat(_, _) => {
                    source.push_str("    gust_io_println(");
                    push_c_value(&mut source, value);
                    source.push_str(");\n");
                }
                LoweredExprKind::BoolLiteral(_) | LoweredExprKind::NumberLiteral(_) => {
                    unreachable!("println only lowers String values")
                }
            },
        }
    }

    source.push_str("    return 0;\n}\n");
    source
}

fn push_c_local_name(source: &mut String, name: &str) {
    source.push_str("gust_");
    source.push_str(name);
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
            source.push_str("gust_string_concat(");
            push_c_value(source, left);
            source.push_str(", ");
            push_c_value(source, right);
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
