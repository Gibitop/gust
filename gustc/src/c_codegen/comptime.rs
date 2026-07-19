fn push_c_comptime_runtime(source: &mut String) {
    source.push_str(
        "static FILE* gust_comptime_file = NULL;\n\
static void gust_comptime_fail(void) {\n\
    fputs(\"failed to write comptime result\\n\", stderr);\n\
    exit(125);\n\
}\n\
static void gust_comptime_write_byte(uint8_t value) {\n\
    if (fwrite(&value, 1, 1, gust_comptime_file) != 1) gust_comptime_fail();\n\
}\n\
static void gust_comptime_write_u32(uint32_t value) {\n\
    for (int index = 0; index < 4; index++) gust_comptime_write_byte((uint8_t)(value >> (index * 8)));\n\
}\n\
static void gust_comptime_write_u64(uint64_t value) {\n\
    for (int index = 0; index < 8; index++) gust_comptime_write_byte((uint8_t)(value >> (index * 8)));\n\
}\n\
static void gust_comptime_write_bytes(const void* data, size_t len) {\n\
    if (len > 0 && fwrite(data, 1, len, gust_comptime_file) != len) gust_comptime_fail();\n\
}\n\
static void gust_comptime_write_string_bytes(const unsigned char* data, size_t len) {\n\
    gust_comptime_write_u64((uint64_t)len);\n\
    gust_comptime_write_bytes(data, len);\n\
}\n\
static void gust_comptime_write_c_string(const char* value) {\n\
    gust_comptime_write_string_bytes((const unsigned char*)value, strlen(value));\n\
}\n\n",
    );
}

fn push_c_comptime_result_write(
    source: &mut String,
    program: &LoweredProgram,
    options: &CComptimeOptions,
) {
    let Some(entry) = program
        .functions
        .iter()
        .find(|function| function.name == options.entry_name)
    else {
        source.push_str("    fputs(\"missing comptime entry function\\n\", stderr);\n");
        source.push_str("    return 126;\n");
        return;
    };

    source.push_str("    gust_comptime_file = fopen(\"");
    push_c_string_value(source, &options.result_path);
    source.push_str("\", \"wb\");\n");
    source.push_str("    if (gust_comptime_file == NULL) {\n");
    source.push_str("        fputs(\"failed to open comptime result artifact\\n\", stderr);\n");
    source.push_str("        return 125;\n");
    source.push_str("    }\n");
    source.push_str("    gust_comptime_write_bytes(\"GCT1\", 4);\n");

    if entry.return_type == LoweredType::Void {
        source.push_str("    ");
        push_c_function_name(source, &entry.name);
        source.push_str("();\n");
        source.push_str("    gust_comptime_write_byte(0);\n");
    } else {
        source.push_str("    ");
        push_c_type(source, &entry.return_type);
        source.push_str(" gust_comptime_result = ");
        push_c_function_name(source, &entry.name);
        source.push_str("();\n");
        push_c_comptime_serialize_value(
            source,
            &entry.return_type,
            "gust_comptime_result",
            program,
            1,
        );
    }

    source.push_str("gust_comptime_serialized:\n");
    source.push_str("    if (fclose(gust_comptime_file) != 0) gust_comptime_fail();\n");
}

fn push_c_comptime_serialize_value(
    source: &mut String,
    type_: &LoweredType,
    value: &str,
    program: &LoweredProgram,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    match type_ {
        LoweredType::Void => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(0);\n");
        }
        LoweredType::Basic(BasicType::Bool) => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(1);\n");
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(");
            source.push_str(value);
            source.push_str(" ? 1 : 0);\n");
        }
        LoweredType::Basic(BasicType::String) => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(3);\n");
            source.push_str(&pad);
            source.push_str("gust_comptime_write_string_bytes(");
            source.push_str(value);
            source.push_str(".gust_data, ");
            source.push_str(value);
            source.push_str(".gust_byte_len);\n");
        }
        LoweredType::Basic(BasicType::Char) => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(4);\n");
            source.push_str(&pad);
            source.push_str("gust_comptime_write_u32(");
            source.push_str(value);
            source.push_str(");\n");
        }
        LoweredType::Basic(type_) => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(2);\n");
            source.push_str(&pad);
            source.push_str("gust_comptime_write_c_string(\"");
            source.push_str(type_.name());
            source.push_str("\");\n");
            source.push_str(&pad);
            source.push_str("{ char gust_comptime_number[128];\n");
            source.push_str(&pad);
            source.push_str("  snprintf(gust_comptime_number, sizeof(gust_comptime_number), ");
            source.push_str(c_comptime_number_format(*type_));
            source.push_str(", ");
            source.push_str(c_comptime_number_cast(*type_));
            source.push_str(value);
            source.push_str(");\n");
            source.push_str(&pad);
            source.push_str("  gust_comptime_write_c_string(gust_comptime_number); }\n");
        }
        LoweredType::Struct(name) => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(6);\n");
            source.push_str(&pad);
            source.push_str("gust_comptime_write_c_string(\"");
            push_c_string_value(source, name);
            source.push_str("\");\n");
            let Some(struct_) = program.structs.iter().find(|struct_| struct_.name == *name)
            else {
                source.push_str(&pad);
                source.push_str("gust_comptime_write_u64(0);\n");
                return;
            };
            source.push_str(&pad);
            source.push_str("gust_comptime_write_u64(");
            source.push_str(&struct_.fields.len().to_string());
            source.push_str(");\n");
            for field in &struct_.fields {
                source.push_str(&pad);
                source.push_str("gust_comptime_write_c_string(\"");
                push_c_string_value(source, &field.name);
                source.push_str("\");\n");
                let field_value = format!("{value}->gust_{}", sanitized_name(&field.name));
                push_c_comptime_serialize_value(source, &field.type_, &field_value, program, indent);
            }
        }
        LoweredType::Enum(name) => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(7);\n");
            source.push_str(&pad);
            source.push_str("gust_comptime_write_c_string(\"");
            push_c_string_value(source, name);
            source.push_str("\");\n");
            let Some(enum_) = program.enums.iter().find(|enum_| enum_.name == *name) else {
                source.push_str(&pad);
                source.push_str("gust_comptime_write_c_string(\"\");\n");
                source.push_str(&pad);
                source.push_str("gust_comptime_write_byte(0);\n");
                return;
            };
            source.push_str(&pad);
            source.push_str("switch (");
            source.push_str(value);
            source.push_str(".gust_tag) {\n");
            for variant in &enum_.variants {
                source.push_str(&pad);
                source.push_str("case ");
                push_c_enum_variant_tag(source, name, &variant.name);
                source.push_str(":\n");
                source.push_str(&pad);
                source.push_str("    gust_comptime_write_c_string(\"");
                push_c_string_value(source, &variant.name);
                source.push_str("\");\n");
                match &variant.payload {
                    Some(payload) => {
                        source.push_str(&pad);
                        source.push_str("    gust_comptime_write_byte(1);\n");
                        let payload_value =
                            format!("{value}.gust_payload.gust_{}", sanitized_name(&variant.name));
                        push_c_comptime_serialize_value(
                            source,
                            payload,
                            &payload_value,
                            program,
                            indent + 1,
                        );
                    }
                    None => {
                        source.push_str(&pad);
                        source.push_str("    gust_comptime_write_byte(0);\n");
                    }
                }
                source.push_str(&pad);
                source.push_str("    break;\n");
            }
            source.push_str(&pad);
            source.push_str("}\n");
        }
        LoweredType::Function { .. } => {
            push_c_comptime_serialize_function(source, type_, value, program, indent);
        }
        LoweredType::Trait(_) => {
            source.push_str(&pad);
            source.push_str("gust_comptime_write_byte(255);\n");
            source.push_str(&pad);
            source.push_str("gust_comptime_write_c_string(\"comptime result cannot be materialized as Gust source\");\n");
        }
    }
}

fn push_c_comptime_serialize_function(
    source: &mut String,
    type_: &LoweredType,
    value: &str,
    program: &LoweredProgram,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    let _ = type_;
    let matching = program.closure_functions.iter().collect::<Vec<_>>();

    for function in matching {
        source.push_str(&pad);
        source.push_str("if (");
        source.push_str(value);
        source.push_str(".gust_call == ");
        push_c_function_name(source, &function.name);
        source.push_str(") {\n");
        source.push_str(&pad);
        source.push_str("    gust_comptime_write_byte(9);\n");
        source.push_str(&pad);
        source.push_str("    gust_comptime_write_c_string(\"");
        push_c_string_value(source, &function.name);
        source.push_str("\");\n");
        source.push_str(&pad);
        source.push_str("    gust_comptime_write_u64(");
        source.push_str(&function.captures.len().to_string());
        source.push_str(");\n");

        if !function.captures.is_empty() {
            let env_type = closure_env_type_name(&function.name);
            source.push_str(&pad);
            source.push_str("    ");
            source.push_str(&env_type);
            source.push_str("* gust_comptime_env = ");
            source.push_str(value);
            source.push_str(".gust_env;\n");
            for capture in &function.captures {
                source.push_str(&pad);
                source.push_str("    gust_comptime_write_c_string(\"");
                push_c_string_value(source, &capture.name);
                source.push_str("\");\n");
                let capture_value = format!(
                    "(*gust_comptime_env->gust_{})",
                    sanitized_name(&capture.name)
                );
                push_c_comptime_serialize_value(
                    source,
                    &capture.type_,
                    &capture_value,
                    program,
                    indent + 1,
                );
            }
        }

        source.push_str(&pad);
        source.push_str("    goto gust_comptime_serialized;\n");
        source.push_str(&pad);
        source.push_str("}\n");
    }

    source.push_str(&pad);
    source.push_str("gust_comptime_write_byte(255);\n");
    source.push_str(&pad);
    source.push_str("gust_comptime_write_c_string(\"comptime result cannot be materialized as Gust source\");\n");
}

fn c_comptime_number_format(type_: BasicType) -> &'static str {
    match type_ {
        BasicType::F32 | BasicType::F64 => "\"%.17g\"",
        BasicType::U8 | BasicType::U16 | BasicType::U32 | BasicType::U64 | BasicType::U128 | BasicType::Usize => "\"%llu\"",
        BasicType::I8 | BasicType::I16 | BasicType::I32 | BasicType::I64 | BasicType::I128 => "\"%lld\"",
        BasicType::String | BasicType::Char | BasicType::Bool => unreachable!(),
    }
}

fn c_comptime_number_cast(type_: BasicType) -> &'static str {
    match type_ {
        BasicType::F32 | BasicType::F64 => "(double)",
        BasicType::U8 | BasicType::U16 | BasicType::U32 | BasicType::U64 | BasicType::U128 | BasicType::Usize => "(unsigned long long)",
        BasicType::I8 | BasicType::I16 | BasicType::I32 | BasicType::I64 | BasicType::I128 => "(long long)",
        BasicType::String | BasicType::Char | BasicType::Bool => unreachable!(),
    }
}
