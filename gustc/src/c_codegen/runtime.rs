fn is_string_builder_name(name: &str) -> bool {
    name == "StringBuilder" || name.ends_with("::StringBuilder")
}

fn string_builder_method(name: &str) -> Option<&str> {
    ["withCapacity", "append", "build"]
        .into_iter()
        .find(|method| name.ends_with(&format!("StringBuilder.{method}")))
}

fn push_c_string_builder_helpers(source: &mut String, structs: &[LoweredStruct]) {
    for struct_ in structs
        .iter()
        .filter(|struct_| is_string_builder_name(&struct_.name))
    {
        source.push_str("static void gust_rt_string_builder_append_");
        push_c_struct_name(source, &struct_.name);
        source.push('(');
        push_c_struct_name(source, &struct_.name);
        source.push_str("* builder, gust_rt_string value) {\n");
        source.push_str("    size_t required = builder->gust_length + value.gust_byte_len;\n");
        source.push_str("    if (required > builder->gust_capacity) {\n");
        source.push_str("        size_t capacity = builder->gust_capacity == 0 ? 16 : builder->gust_capacity * 2;\n");
        source.push_str(
            "        while (capacity < required) {\n            capacity *= 2;\n        }\n",
        );
        source.push_str("        unsigned char* data = gust_rt_alloc(capacity);\n");
        source.push_str("        if (builder->gust_length > 0) {\n            memcpy(data, builder->gust_data, builder->gust_length);\n        }\n");
        source.push_str(
            "        builder->gust_data = data;\n        builder->gust_capacity = capacity;\n",
        );
        source.push_str("    }\n");
        source.push_str("    if (value.gust_byte_len > 0) {\n        memcpy((unsigned char*)builder->gust_data + builder->gust_length, value.gust_data, value.gust_byte_len);\n    }\n");
        source.push_str("    builder->gust_length = required;\n");
        source.push_str("}\n\n");

        source.push_str("static gust_rt_string gust_rt_string_builder_build_");
        push_c_struct_name(source, &struct_.name);
        source.push('(');
        push_c_struct_name(source, &struct_.name);
        source.push_str("* builder) {\n");
        source.push_str("    unsigned char* data = gust_rt_alloc(builder->gust_length == 0 ? 1 : builder->gust_length);\n");
        source.push_str("    if (builder->gust_length > 0) {\n        memcpy(data, builder->gust_data, builder->gust_length);\n    }\n");
        source.push_str("    return (gust_rt_string){ .gust_data = data, .gust_byte_len = builder->gust_length };\n");
        source.push_str("}\n\n");
    }
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
        if struct_.raw_buffer_element.is_some() || is_string_builder_name(&struct_.name) {
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

        if is_string_builder_name(&struct_.name) {
            source.push_str("    result->gust_capacity = value->gust_capacity;\n");
            source.push_str("    result->gust_length = value->gust_length;\n");
            source.push_str("    result->gust_data = NULL;\n");
            source.push_str("    if (value->gust_capacity > 0) {\n        result->gust_data = gust_rt_alloc(value->gust_capacity);\n    }\n");
            source.push_str("    if (value->gust_length > 0) {\n        memcpy(result->gust_data, value->gust_data, value->gust_length);\n    }\n");
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

fn push_c_float_to_int_cast_name(source: &mut String, source_type: BasicType, target_type: BasicType) {
    source.push_str("gust_rt_");
    source.push_str(source_type.name());
    source.push_str("_to_");
    source.push_str(target_type.name());
}

fn push_c_float_to_int_cast_helper(
    source: &mut String,
    source_type: BasicType,
    target_type: BasicType,
) {
    source.push_str("static ");
    source.push_str(c_basic_type(target_type));
    source.push(' ');
    push_c_float_to_int_cast_name(source, source_type, target_type);
    source.push('(');
    source.push_str(c_basic_type(source_type));
    source.push_str(" value) {\n");
    source.push_str("    if (value != value) {\n");
    source.push_str("        return 0;\n");
    source.push_str("    }\n");
    source.push_str("    long double widened = (long double)value;\n");
    source.push_str("    if (widened <= ");
    push_c_float_to_int_min_bound(source, target_type);
    source.push_str(") {\n");
    source.push_str("        return ");
    push_c_integer_min_value(source, target_type);
    source.push_str(";\n");
    source.push_str("    }\n");
    source.push_str("    if (widened >= ");
    push_c_float_to_int_max_bound(source, target_type);
    source.push_str(") {\n");
    source.push_str("        return ");
    push_c_integer_max_value(source, target_type);
    source.push_str(";\n");
    source.push_str("    }\n");
    source.push_str("    return (");
    source.push_str(c_basic_type(target_type));
    source.push_str(")value;\n");
    source.push_str("}\n\n");
}

fn push_c_float_to_int_min_bound(source: &mut String, type_: BasicType) {
    if is_unsigned_integer_type(type_) {
        source.push_str("0.0L");
    } else {
        source.push_str("(long double)(");
        push_c_integer_min_value(source, type_);
        source.push(')');
    }
}

fn push_c_float_to_int_max_bound(source: &mut String, type_: BasicType) {
    source.push_str("(long double)(");
    push_c_integer_max_value(source, type_);
    source.push(')');
}

fn push_c_integer_min_value(source: &mut String, type_: BasicType) {
    match type_ {
        BasicType::U8
        | BasicType::U16
        | BasicType::U32
        | BasicType::U64
        | BasicType::U128
        | BasicType::Usize => source.push('0'),
        BasicType::I8 => source.push_str("INT8_MIN"),
        BasicType::I16 => source.push_str("INT16_MIN"),
        BasicType::I32 => source.push_str("INT32_MIN"),
        BasicType::I64 => source.push_str("INT64_MIN"),
        BasicType::I128 => source.push_str("(-((__int128)(((unsigned __int128)-1) >> 1)) - 1)"),
        BasicType::String | BasicType::Char | BasicType::Bool | BasicType::F32 | BasicType::F64 => {
            unreachable!("only integer cast targets have integer bounds")
        }
    }
}

fn push_c_integer_max_value(source: &mut String, type_: BasicType) {
    match type_ {
        BasicType::U8 => source.push_str("UINT8_MAX"),
        BasicType::U16 => source.push_str("UINT16_MAX"),
        BasicType::U32 => source.push_str("UINT32_MAX"),
        BasicType::U64 => source.push_str("UINT64_MAX"),
        BasicType::U128 => source.push_str("((unsigned __int128)-1)"),
        BasicType::Usize => source.push_str("((size_t)-1)"),
        BasicType::I8 => source.push_str("INT8_MAX"),
        BasicType::I16 => source.push_str("INT16_MAX"),
        BasicType::I32 => source.push_str("INT32_MAX"),
        BasicType::I64 => source.push_str("INT64_MAX"),
        BasicType::I128 => source.push_str("((__int128)(((unsigned __int128)-1) >> 1))"),
        BasicType::String | BasicType::Char | BasicType::Bool | BasicType::F32 | BasicType::F64 => {
            unreachable!("only integer cast targets have integer bounds")
        }
    }
}

fn is_unsigned_integer_type(type_: BasicType) -> bool {
    matches!(
        type_,
        BasicType::U8
            | BasicType::U16
            | BasicType::U32
            | BasicType::U64
            | BasicType::U128
            | BasicType::Usize
    )
}

fn push_c_number_to_string_helper(source: &mut String, type_: BasicType) {
    if type_ == BasicType::U128 {
        source
            .push_str("static gust_rt_string gust_rt_u128_to_string(unsigned __int128 value) {\n");
        source.push_str("    char buffer[40];\n");
        source.push_str("    char* cursor = buffer + sizeof(buffer);\n");
        source.push_str("    do {\n");
        source.push_str("        *--cursor = (char)('0' + value % 10);\n");
        source.push_str("        value /= 10;\n");
        source.push_str("    } while (value != 0);\n");
        source.push_str("    size_t length = (buffer + sizeof(buffer)) - cursor;\n");
        source.push_str("    unsigned char* data = gust_rt_alloc(length);\n");
        source.push_str("    memcpy(data, cursor, length);\n");
        source.push_str(
            "    return (gust_rt_string){ .gust_data = data, .gust_byte_len = length };\n",
        );
        source.push_str("}\n\n");
        return;
    }

    if type_ == BasicType::I128 {
        source.push_str("static gust_rt_string gust_rt_i128_to_string(__int128 value) {\n");
        source.push_str("    bool negative = value < 0;\n");
        source.push_str("    unsigned __int128 magnitude = negative\n");
        source.push_str("        ? (unsigned __int128)(-(value + 1)) + 1\n");
        source.push_str("        : (unsigned __int128)value;\n");
        source.push_str("    char buffer[41];\n");
        source.push_str("    char* cursor = buffer + sizeof(buffer);\n");
        source.push_str("    do {\n");
        source.push_str("        *--cursor = (char)('0' + magnitude % 10);\n");
        source.push_str("        magnitude /= 10;\n");
        source.push_str("    } while (magnitude != 0);\n");
        source.push_str("    if (negative) {\n");
        source.push_str("        *--cursor = '-';\n");
        source.push_str("    }\n");
        source.push_str("    size_t length = (buffer + sizeof(buffer)) - cursor;\n");
        source.push_str("    unsigned char* data = gust_rt_alloc(length);\n");
        source.push_str("    memcpy(data, cursor, length);\n");
        source.push_str(
            "    return (gust_rt_string){ .gust_data = data, .gust_byte_len = length };\n",
        );
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
        BasicType::String
        | BasicType::Char
        | BasicType::Bool
        | BasicType::U128
        | BasicType::I128 => {
            unreachable!("only directly formatted numeric types reach this path")
        }
    };

    source.push_str("static gust_rt_string gust_rt_");
    source.push_str(type_.name());
    source.push_str("_to_string(");
    source.push_str(c_basic_type(type_));
    source.push_str(" value) {\n");
    source.push_str("    int length = snprintf(NULL, 0, \"");
    source.push_str(format);
    source.push_str("\", ");
    source.push_str(cast);
    source.push_str(");\n");
    source.push_str("    unsigned char* data = gust_rt_alloc((size_t)length + 1);\n");
    source.push_str("    snprintf((char*)data, (size_t)length + 1, \"");
    source.push_str(format);
    source.push_str("\", ");
    source.push_str(cast);
    source.push_str(");\n");
    source.push_str(
        "    return (gust_rt_string){ .gust_data = data, .gust_byte_len = (size_t)length };\n",
    );
    source.push_str("}\n\n");
}
