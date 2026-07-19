use std::collections::HashSet;

use crate::ast::{BasicType, BinaryOp};
use crate::lower::{
    LoweredClosureFunction, LoweredEnum, LoweredExpr, LoweredExprKind, LoweredFunction,
    LoweredMatchBindSource, LoweredMatchDecision, LoweredMatchTest, LoweredProgram,
    LoweredSourceLocation, LoweredStatement, LoweredStaticVar, LoweredStruct, LoweredType,
};

#[derive(Debug, Clone, Default)]
pub struct CCodegenOptions {
    pub gc_stress: bool,
}

#[derive(Debug, Clone)]
pub struct CComptimeOptions {
    pub entry_name: String,
    pub result_path: String,
}

pub fn emit_c(program: &LoweredProgram) -> String {
    emit_c_with_options(program, CCodegenOptions::default())
}

pub fn emit_c_with_options(program: &LoweredProgram, options: CCodegenOptions) -> String {
    emit_c_internal(program, options, None)
}

pub fn emit_c_for_comptime(
    program: &LoweredProgram,
    options: CCodegenOptions,
    comptime: CComptimeOptions,
) -> String {
    emit_c_internal(program, options, Some(comptime))
}

fn emit_c_internal(
    program: &LoweredProgram,
    options: CCodegenOptions,
    comptime: Option<CComptimeOptions>,
) -> String {
    let uses_string = comptime.is_some() || program_uses_type(program, BasicType::String);
    let uses_string_equality = program_uses_string_equality(program);
    let number_to_string_types = number_to_string_types(program);
    let float_to_int_casts = float_to_int_casts(program);
    let uses_string_concat = program_uses_string_concat(program);
    let uses_string_builder = program
        .structs
        .iter()
        .any(|struct_| is_string_builder_name(&struct_.name));
    let uses_number_to_string = !number_to_string_types.is_empty();
    let uses_enum_trait_object = program_uses_enum_trait_object(program);
    let uses_alloc = uses_string_concat
        || uses_string_builder
        || uses_number_to_string
        || uses_enum_trait_object
        || !program.structs.is_empty()
        || !program.closure_functions.is_empty();
    let uses_bool = uses_alloc
        || program_uses_type(program, BasicType::Bool)
        || uses_string_equality
        || number_to_string_types.contains(&BasicType::I128)
        || program_uses_match_or(program)
        || !float_to_int_casts.is_empty();
    let uses_usize = comptime.is_some()
        || uses_alloc
        || uses_string
        || program_uses_type(program, BasicType::Usize)
        || program
            .structs
            .iter()
            .any(|struct_| struct_.raw_buffer_element.is_some());
    let uses_float =
        program_uses_type(program, BasicType::F32) || program_uses_type(program, BasicType::F64);
    let uses_fixed_width_int = program_uses_fixed_width_int(program) || comptime.is_some();
    let uses_panic = program_uses_panic(program);
    let uses_println = comptime.is_some()
        || program.statements.iter().any(statement_uses_println)
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

    if uses_println || uses_number_to_string || uses_panic || comptime.is_some() {
        source.push_str("#include <stdio.h>\n");
    }

    if uses_float {
        source.push_str("#include <math.h>\n");
    }

    if uses_alloc || uses_panic || comptime.is_some() {
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

    if uses_alloc {
        push_c_closure_env_structs(&mut source, &program.closure_functions);
        push_c_gc_runtime(&mut source, options.gc_stress);
        push_c_gc_descriptors(&mut source, program);
    }

    if uses_string_concat {
        source.push_str(
            "static gust_rt_string gust_rt_string_concat(gust_rt_string left, gust_rt_string right) {\n",
        );
        source.push_str("    size_t byte_len = left.gust_byte_len + right.gust_byte_len;\n");
        source.push_str("    unsigned char* data = gust_rt_alloc(&gust_rt_desc_bytes, byte_len == 0 ? 1 : byte_len);\n");
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

    for (source_type, target_type) in float_to_int_casts {
        push_c_float_to_int_cast_helper(&mut source, source_type, target_type);
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

    if uses_panic {
        push_c_panic_runtime(&mut source);
    }

    push_c_string_builder_helpers(&mut source, &program.structs);

    push_c_struct_runtime_helpers(&mut source, program);

    if comptime.is_some() {
        push_c_comptime_runtime(&mut source);
    }

    push_c_static_declarations(&mut source, &program.statics);

    for function in ordered_functions(&program.functions) {
        push_c_function_signature(&mut source, function);
        source.push_str(";\n");
    }
    if !program.functions.is_empty() {
        source.push('\n');
    }

    for function in &program.closure_functions {
        push_c_closure_function_signature(&mut source, function);
        source.push_str(";\n");
    }
    if !program.closure_functions.is_empty() {
        source.push('\n');
    }

    push_c_trait_dispatch_helpers(&mut source, program);

    for function in ordered_functions(&program.functions) {
        push_c_function(
            &mut source,
            function,
            &program.structs,
            uses_panic,
            uses_alloc,
        );
        source.push('\n');
    }

    for function in &program.closure_functions {
        push_c_closure_function(
            &mut source,
            function,
            &program.structs,
            uses_panic,
            uses_alloc,
        );
        source.push('\n');
    }

    source.push_str("int main(void) {\n");

    if uses_panic {
        push_c_stack_push(&mut source, "main", &program.main_location, 1);
    }
    if uses_alloc {
        push_c_static_roots(&mut source, &program.statics, 1);
        source.push_str("    gust_rt_root_slot* gust_rt_function_roots = gust_rt_roots;\n");
    }

    for statement in &program.statements {
        push_c_statement(
            &mut source,
            statement,
            1,
            &program.structs,
            uses_panic,
            uses_alloc,
            None,
        );
    }

    if let Some(comptime) = comptime {
        push_c_comptime_result_write(&mut source, program, &comptime);
    }

    if uses_alloc {
        source.push_str("    gust_rt_roots_pop_to(gust_rt_function_roots);\n");
    }
    if uses_panic {
        source.push_str("    gust_rt_stack_pop();\n");
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
include!("comptime.rs");
