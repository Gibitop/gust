use std::collections::HashSet;

use crate::ast::{BasicType, BinaryOp};
use crate::lower::{
    LoweredClosureFunction, LoweredEnum, LoweredExpr, LoweredExprKind, LoweredFunction,
    LoweredPattern, LoweredProgram, LoweredStatement, LoweredStruct, LoweredType,
};

pub fn emit_c(program: &LoweredProgram) -> String {
    let uses_string = program_uses_type(program, BasicType::String);
    let uses_string_equality = program_uses_string_equality(program);
    let number_to_string_types = number_to_string_types(program);
    let uses_bool = program_uses_type(program, BasicType::Bool)
        || uses_string_equality
        || number_to_string_types.contains(&BasicType::I128);
    let uses_usize = uses_string
        || program_uses_type(program, BasicType::Usize)
        || program
            .structs
            .iter()
            .any(|struct_| struct_.raw_buffer_element.is_some());
    let uses_float =
        program_uses_type(program, BasicType::F32) || program_uses_type(program, BasicType::F64);
    let uses_fixed_width_int = program_uses_fixed_width_int(program);
    let uses_string_concat = program_uses_string_concat(program);
    let uses_string_builder = program
        .structs
        .iter()
        .any(|struct_| is_string_builder_name(&struct_.name));
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
        || uses_string_builder
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

    if uses_string {
        source.push_str("typedef struct {\n");
        source.push_str("    const unsigned char* gust_data;\n");
        source.push_str("    size_t gust_byte_len;\n");
        source.push_str("} gust_rt_string;\n\n");
        source.push_str("static size_t gust_rt_string_char_len(gust_rt_string value) {\n");
        source.push_str("    size_t length = 0;\n");
        source.push_str("    for (size_t index = 0; index < value.gust_byte_len; index++) {\n");
        source.push_str("        if ((value.gust_data[index] & 0xc0) != 0x80) {\n            length++;\n        }\n");
        source.push_str("    }\n    return length;\n}\n\n");
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
            "static gust_rt_string gust_rt_string_concat(gust_rt_string left, gust_rt_string right) {\n",
        );
        source.push_str("    size_t byte_len = left.gust_byte_len + right.gust_byte_len;\n");
        source.push_str("    unsigned char* data = gust_rt_alloc(byte_len == 0 ? 1 : byte_len);\n");
        source.push_str("    memcpy(data, left.gust_data, left.gust_byte_len);\n");
        source.push_str(
            "    memcpy(data + left.gust_byte_len, right.gust_data, right.gust_byte_len);\n",
        );
        source.push_str(
            "    return (gust_rt_string){ .gust_data = data, .gust_byte_len = byte_len };\n",
        );
        source.push_str("}\n\n");
    }

    for type_ in number_to_string_types {
        push_c_number_to_string_helper(&mut source, type_);
    }

    if uses_string_equality {
        source.push_str(
            "static bool gust_rt_string_equal(gust_rt_string left, gust_rt_string right) {\n",
        );
        source.push_str("    return left.gust_byte_len == right.gust_byte_len\n");
        source.push_str(
            "        && memcmp(left.gust_data, right.gust_data, left.gust_byte_len) == 0;\n",
        );
        source.push_str("}\n\n");
    }

    if uses_println {
        source.push_str("static void gust_rt_io_println(gust_rt_string value) {\n");
        source.push_str("    fwrite(value.gust_data, 1, value.gust_byte_len, stdout);\n");
        source.push_str("    fputc('\\n', stdout);\n");
        source.push_str("}\n\n");
    }

    push_c_string_builder_helpers(&mut source, &program.structs);

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

include!("analysis.rs");
include!("types.rs");
include!("runtime.rs");
include!("items.rs");
include!("statements.rs");
include!("expressions.rs");
include!("names.rs");
