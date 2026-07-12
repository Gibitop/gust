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

    if struct_.raw_buffer_element.is_some() || is_string_builder_name(&struct_.name) {
        source.push_str("    void* gust_data;\n");
        source.push_str("    size_t gust_capacity;\n");
        source.push_str("    size_t gust_length;\n");
    }
    if struct_.fields.is_empty()
        && struct_.raw_buffer_element.is_none()
        && !is_string_builder_name(&struct_.name)
    {
        source.push_str("    bool gust_empty;\n");
    }

    source.push_str("};\n");
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
    source.push('_');
    push_c_identifier_suffix(source, type_name);
    source.push('_');
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
