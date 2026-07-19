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
        source.push_str("        unsigned char* data = gust_rt_alloc(&gust_rt_desc_bytes, capacity);\n");
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
        source.push_str("    unsigned char* data = gust_rt_alloc(&gust_rt_desc_bytes, builder->gust_length == 0 ? 1 : builder->gust_length);\n");
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

fn push_c_gc_runtime(source: &mut String, gc_stress: bool) {
    source.push_str("typedef struct gust_rt_type_desc {\n");
    source.push_str("    const char* gust_name;\n");
    source.push_str("    void (*gust_trace)(void* value);\n");
    source.push_str("} gust_rt_type_desc;\n\n");
    source.push_str("typedef struct gust_rt_object_header {\n");
    source.push_str("    const gust_rt_type_desc* gust_desc;\n");
    source.push_str("    bool gust_marked;\n");
    source.push_str("    size_t gust_size;\n");
    source.push_str("    struct gust_rt_object_header* gust_next;\n");
    source.push_str("} gust_rt_object_header;\n\n");
    source.push_str("typedef struct gust_rt_root_slot {\n");
    source.push_str("    void* gust_value;\n");
    source.push_str("    void (*gust_trace)(void* value);\n");
    source.push_str("    struct gust_rt_root_slot* gust_previous;\n");
    source.push_str("} gust_rt_root_slot;\n\n");
    source.push_str("static gust_rt_object_header* gust_rt_heap_objects = NULL;\n");
    source.push_str("static gust_rt_root_slot* gust_rt_roots = NULL;\n");
    source.push_str("static size_t gust_rt_heap_bytes = 0;\n");
    source.push_str("static size_t gust_rt_next_collection_bytes = 1024 * 1024;\n\n");
    source.push_str("static void gust_rt_mark(void* value);\n\n");
    source.push_str("static void gust_rt_root_push(gust_rt_root_slot* slot) {\n");
    source.push_str("    slot->gust_previous = gust_rt_roots;\n");
    source.push_str("    gust_rt_roots = slot;\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_roots_pop_to(gust_rt_root_slot* slot) {\n");
    source.push_str("    gust_rt_roots = slot;\n");
    source.push_str("}\n\n");
    source.push_str("static gust_rt_object_header* gust_rt_find_header(void* value) {\n");
    source.push_str("    for (gust_rt_object_header* header = gust_rt_heap_objects; header != NULL; header = header->gust_next) {\n");
    source.push_str("        if ((void*)(header + 1) == value) {\n");
    source.push_str("            return header;\n");
    source.push_str("        }\n");
    source.push_str("    }\n");
    source.push_str("    return NULL;\n");
    source.push_str("}\n\n");
    source.push_str("static void* gust_rt_alloc(const gust_rt_type_desc* desc, size_t size) {\n");
    source.push_str("    size_t payload_size = size == 0 ? 1 : size;\n");
    source.push_str("    gust_rt_object_header* header = malloc(sizeof(gust_rt_object_header) + payload_size);\n");
    source.push_str("    if (header == NULL) {\n");
    source.push_str("        abort();\n");
    source.push_str("    }\n");
    source.push_str("    header->gust_desc = desc;\n");
    source.push_str("    header->gust_marked = false;\n");
    source.push_str("    header->gust_size = payload_size;\n");
    source.push_str("    header->gust_next = gust_rt_heap_objects;\n");
    source.push_str("    gust_rt_heap_objects = header;\n");
    source.push_str("    gust_rt_heap_bytes += payload_size;\n");
    source.push_str("    return (void*)(header + 1);\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_mark(void* value) {\n");
    source.push_str("    if (value == NULL) {\n");
    source.push_str("        return;\n");
    source.push_str("    }\n");
    source.push_str("    gust_rt_object_header* header = gust_rt_find_header(value);\n");
    source.push_str("    if (header == NULL) {\n");
    source.push_str("        return;\n");
    source.push_str("    }\n");
    source.push_str("    if (header->gust_marked) {\n");
    source.push_str("        return;\n");
    source.push_str("    }\n");
    source.push_str("    header->gust_marked = true;\n");
    source.push_str("    if (header->gust_desc != NULL && header->gust_desc->gust_trace != NULL) {\n");
    source.push_str("        header->gust_desc->gust_trace(value);\n");
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_mark_roots(void) {\n");
    source.push_str("    for (gust_rt_root_slot* slot = gust_rt_roots; slot != NULL; slot = slot->gust_previous) {\n");
    source.push_str("        if (slot->gust_trace != NULL) {\n");
    source.push_str("            slot->gust_trace(slot->gust_value);\n");
    source.push_str("        }\n");
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_trace_heap_object_root(void* value) {\n");
    source.push_str("    gust_rt_mark(*(void**)value);\n");
    source.push_str("}\n\n");
    source.push_str("static const bool gust_rt_gc_stress = ");
    if gc_stress {
        source.push_str("true");
    } else {
        source.push_str("false");
    }
    source.push_str(";\n\n");
    source.push_str("static void gust_rt_collect(void) {\n");
    source.push_str("    gust_rt_object_header** current = &gust_rt_heap_objects;\n");
    source.push_str("    while (*current != NULL) {\n");
    source.push_str("        gust_rt_object_header* header = *current;\n");
    source.push_str("        if (header->gust_marked) {\n");
    source.push_str("            header->gust_marked = false;\n");
    source.push_str("            current = &header->gust_next;\n");
    source.push_str("        } else {\n");
    source.push_str("            *current = header->gust_next;\n");
    source.push_str("            gust_rt_heap_bytes -= header->gust_size;\n");
    source.push_str("            free(header);\n");
    source.push_str("        }\n");
    source.push_str("    }\n");
    source.push_str("    if (gust_rt_next_collection_bytes < 1024 * 1024) {\n");
    source.push_str("        gust_rt_next_collection_bytes = 1024 * 1024;\n");
    source.push_str("    }\n");
    source.push_str("    while (gust_rt_next_collection_bytes < gust_rt_heap_bytes * 2) {\n");
    source.push_str("        gust_rt_next_collection_bytes *= 2;\n");
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_safepoint(void) {\n");
    source.push_str("    if (gust_rt_gc_stress || gust_rt_heap_bytes >= gust_rt_next_collection_bytes) {\n");
    source.push_str("        gust_rt_mark_roots();\n");
    source.push_str("        gust_rt_collect();\n");
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_write_ptr(void* owner, void** slot, void* value) {\n");
    source.push_str("    (void)owner;\n");
    source.push_str("    *slot = value;\n");
    source.push_str("}\n\n");
}

fn push_c_gc_descriptors(source: &mut String, program: &LoweredProgram) {
    source.push_str("static void gust_rt_trace_none(void* value) {\n");
    source.push_str("    (void)value;\n");
    source.push_str("}\n\n");
    source.push_str("static const gust_rt_type_desc gust_rt_desc_bytes = { \"bytes\", gust_rt_trace_none };\n");
    source.push_str("static const gust_rt_type_desc gust_rt_desc_clone_entry = { \"clone_entry\", gust_rt_trace_none };\n\n");

    for enum_ in &program.enums {
        source.push_str("static void ");
        push_c_enum_trace_name(source, &enum_.name);
        source.push('(');
        push_c_enum_name(source, &enum_.name);
        source.push_str("* value);\n");
    }
    if !program.enums.is_empty() {
        source.push('\n');
    }

    for struct_ in &program.structs {
        push_c_struct_trace_descriptor(source, struct_);
    }

    for enum_ in &program.enums {
        push_c_enum_trace_descriptor(source, enum_);
    }

    for type_ in gc_cell_types(program)
        .into_iter()
        .filter(|type_| !matches!(type_, LoweredType::Void))
    {
        push_c_cell_trace_descriptor(source, &type_);
    }

    for function in &program.closure_functions {
        if !function.captures.is_empty() {
            push_c_closure_env_trace_descriptor(source, function);
        }
    }
}

fn push_c_struct_trace_descriptor(source: &mut String, struct_: &LoweredStruct) {
    source.push_str("static void ");
    push_c_struct_trace_name(source, &struct_.name);
    source.push_str("(void* object) {\n");
    source.push_str("    ");
    push_c_struct_name(source, &struct_.name);
    source.push_str("* value = object;\n");
    for field in &struct_.fields {
        let mut value = String::new();
        value.push_str("value->");
        push_c_local_name(&mut value, &field.name);
        push_c_trace_value(source, &field.type_, &value);
    }
    if struct_.raw_buffer_element.is_some() || is_string_builder_name(&struct_.name) {
        source.push_str("    gust_rt_mark(value->gust_data);\n");
    }
    if let Some(element) = &struct_.raw_buffer_element {
        source.push_str("    for (size_t gust_index = 0; gust_index < value->gust_length; gust_index++) {\n");
        let mut element_value = String::new();
        element_value.push_str("((");
        push_c_type(&mut element_value, element);
        element_value.push_str("*)value->gust_data)[gust_index]");
        push_c_trace_value(source, element, &element_value);
        source.push_str("    }\n");
    }
    source.push_str("}\n\n");
    source.push_str("static const gust_rt_type_desc ");
    push_c_struct_desc_name(source, &struct_.name);
    source.push_str(" = { \"");
    push_c_string_value(source, &struct_.name);
    source.push_str("\", ");
    push_c_struct_trace_name(source, &struct_.name);
    source.push_str(" };\n\n");
}

fn push_c_enum_trace_descriptor(source: &mut String, enum_: &LoweredEnum) {
    source.push_str("static void ");
    push_c_enum_trace_name(source, &enum_.name);
    source.push('(');
    push_c_enum_name(source, &enum_.name);
    source.push_str("* value) {\n");
    source.push_str("    switch (value->gust_tag) {\n");
    for variant in &enum_.variants {
        source.push_str("        case ");
        push_c_enum_variant_tag(source, &enum_.name, &variant.name);
        source.push_str(":\n");
        if let Some(payload) = &variant.payload {
            let mut value = String::new();
            value.push_str("value->gust_payload.");
            push_c_local_name(&mut value, &variant.name);
            push_c_trace_value(source, payload, &value);
        }
        source.push_str("            break;\n");
    }
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str("static void ");
    push_c_enum_box_trace_name(source, &enum_.name);
    source.push_str("(void* object) {\n");
    source.push_str("    ");
    push_c_enum_trace_name(source, &enum_.name);
    source.push_str("(object);\n");
    source.push_str("}\n\n");
    source.push_str("static const gust_rt_type_desc ");
    push_c_enum_desc_name(source, &enum_.name);
    source.push_str(" = { \"");
    push_c_string_value(source, &enum_.name);
    source.push_str("\", ");
    push_c_enum_box_trace_name(source, &enum_.name);
    source.push_str(" };\n\n");
}

fn push_c_cell_trace_descriptor(source: &mut String, type_: &LoweredType) {
    source.push_str("static void ");
    push_c_cell_trace_name(source, type_);
    source.push_str("(void* object) {\n");
    source.push_str("    ");
    push_c_type(source, type_);
    source.push_str("* value = object;\n");
    push_c_trace_value(source, type_, "(*value)");
    source.push_str("}\n\n");
    source.push_str("static const gust_rt_type_desc ");
    push_c_cell_desc_name(source, type_);
    source.push_str(" = { \"cell\", ");
    push_c_cell_trace_name(source, type_);
    source.push_str(" };\n\n");
}

fn push_c_closure_env_trace_descriptor(source: &mut String, function: &LoweredClosureFunction) {
    let env_type = closure_env_type_name(&function.name);
    source.push_str("static void gust_rt_trace_");
    source.push_str(&env_type);
    source.push_str("(void* object) {\n");
    source.push_str("    ");
    source.push_str(&env_type);
    source.push_str("* value = object;\n");
    for capture in &function.captures {
        source.push_str("    gust_rt_mark(value->");
        push_c_local_name(source, &capture.name);
        source.push_str(");\n");
    }
    source.push_str("}\n\n");
    source.push_str("static const gust_rt_type_desc ");
    push_c_closure_env_desc_name(source, &function.name);
    source.push_str(" = { \"");
    push_c_string_value(source, &env_type);
    source.push_str("\", gust_rt_trace_");
    source.push_str(&env_type);
    source.push_str(" };\n\n");
}

fn push_c_trace_value(source: &mut String, type_: &LoweredType, value: &str) {
    match type_ {
        LoweredType::Basic(BasicType::String) => {
            source.push_str("    gust_rt_mark((void*)");
            source.push_str(value);
            source.push_str(".gust_data);\n");
        }
        LoweredType::Basic(_) | LoweredType::Void => {}
        LoweredType::Struct(_) => {
            source.push_str("    gust_rt_mark(");
            source.push_str(value);
            source.push_str(");\n");
        }
        LoweredType::Enum(name) => {
            source.push_str("    ");
            push_c_enum_trace_name(source, name);
            source.push_str("(&");
            source.push_str(value);
            source.push_str(");\n");
        }
        LoweredType::Trait(_) => {
            source.push_str("    gust_rt_mark(");
            source.push_str(value);
            source.push_str(".gust_self);\n");
        }
        LoweredType::Function { .. } => {
            source.push_str("    gust_rt_mark(");
            source.push_str(value);
            source.push_str(".gust_env);\n");
        }
    }
}

fn gc_cell_types(program: &LoweredProgram) -> Vec<LoweredType> {
    let mut types = Vec::new();
    for static_ in &program.statics {
        types.push(static_.type_.clone());
    }
    for function in &program.functions {
        for param in &function.params {
            types.push(param.type_.clone());
        }
        for statement in &function.statements {
            collect_cell_types_from_statement(statement, &mut types);
        }
        collect_cell_types_from_expr(&function.return_value, &mut types);
    }
    for function in &program.closure_functions {
        for param in &function.params {
            types.push(param.type_.clone());
        }
        for statement in &function.statements {
            collect_cell_types_from_statement(statement, &mut types);
        }
        collect_cell_types_from_expr(&function.return_value, &mut types);
    }
    for statement in &program.statements {
        collect_cell_types_from_statement(statement, &mut types);
    }
    types.sort_by_key(type_name_key);
    types.dedup();
    types
}

fn collect_cell_types_from_statement(statement: &LoweredStatement, types: &mut Vec<LoweredType>) {
    match statement {
        LoweredStatement::Local { value, .. } | LoweredStatement::LocalCell { value, .. } => {
            collect_cell_types_from_expr(value, types);
        }
        LoweredStatement::Assignment { target, value, .. } => {
            collect_cell_types_from_expr(target, types);
            collect_cell_types_from_expr(value, types);
        }
        LoweredStatement::Println(value)
        | LoweredStatement::Expr(value)
        | LoweredStatement::Return(Some(value)) => collect_cell_types_from_expr(value, types),
        LoweredStatement::Panic { message, .. } => collect_cell_types_from_expr(message, types),
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_cell_types_from_expr(condition, types);
            for statement in then_branch {
                collect_cell_types_from_statement(statement, types);
            }
            if let Some(else_branch) = else_branch {
                for statement in else_branch {
                    collect_cell_types_from_statement(statement, types);
                }
            }
        }
        LoweredStatement::While { condition, body } => {
            collect_cell_types_from_expr(condition, types);
            for statement in body {
                collect_cell_types_from_statement(statement, types);
            }
        }
        LoweredStatement::Block(statements) => {
            for statement in statements {
                collect_cell_types_from_statement(statement, types);
            }
        }
        LoweredStatement::Match {
            value, decision, ..
        } => {
            collect_cell_types_from_expr(value, types);
            collect_cell_types_from_decision(decision, types);
        }
        LoweredStatement::Return(None)
        | LoweredStatement::Break
        | LoweredStatement::Continue => {}
    }
}

fn collect_cell_types_from_expr(expr: &LoweredExpr, types: &mut Vec<LoweredType>) {
    types.push(expr.type_.clone());
    match &expr.kind {
        LoweredExprKind::PostfixIncrement(value)
        | LoweredExprKind::Not(value)
        | LoweredExprKind::Negate(value)
        | LoweredExprKind::Clone(value)
        | LoweredExprKind::NumberToString(value) => collect_cell_types_from_expr(value, types),
        LoweredExprKind::StringConcat(left, right)
        | LoweredExprKind::Arithmetic { left, right, .. }
        | LoweredExprKind::Logical { left, right, .. }
        | LoweredExprKind::Comparison { left, right, .. } => {
            collect_cell_types_from_expr(left, types);
            collect_cell_types_from_expr(right, types);
        }
        LoweredExprKind::Cast { value, .. }
        | LoweredExprKind::FieldAccess { object: value, .. } => {
            collect_cell_types_from_expr(value, types)
        }
        LoweredExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                collect_cell_types_from_expr(&field.value, types);
            }
        }
        LoweredExprKind::EnumLiteral { payload, .. } => {
            if let Some(payload) = payload {
                collect_cell_types_from_expr(payload, types);
            }
        }
        LoweredExprKind::Match {
            value, decision, ..
        } => {
            collect_cell_types_from_expr(value, types);
            collect_cell_types_from_decision(decision, types);
        }
        LoweredExprKind::Block { statements, value } => {
            for statement in statements {
                collect_cell_types_from_statement(statement, types);
            }
            collect_cell_types_from_expr(value, types);
        }
        LoweredExprKind::Call { args, .. }
        | LoweredExprKind::CollectionLiteral { items: args, .. } => {
            for arg in args {
                collect_cell_types_from_expr(arg, types);
            }
        }
        LoweredExprKind::TraitObject { value, .. } => collect_cell_types_from_expr(value, types),
        LoweredExprKind::DynamicCall { object, args, .. } => {
            collect_cell_types_from_expr(object, types);
            for arg in args {
                collect_cell_types_from_expr(arg, types);
            }
        }
        LoweredExprKind::IndirectCall { callee, args, .. } => {
            collect_cell_types_from_expr(callee, types);
            for arg in args {
                collect_cell_types_from_expr(arg, types);
            }
        }
        LoweredExprKind::Void
        | LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::Local(_)
        | LoweredExprKind::LocalCell(_)
        | LoweredExprKind::CapturedLocal { .. }
        | LoweredExprKind::Closure { .. } => {}
    }
}

fn collect_cell_types_from_decision(decision: &LoweredMatchDecision, types: &mut Vec<LoweredType>) {
    match decision {
        LoweredMatchDecision::Body { statements, .. } => {
            for statement in statements {
                collect_cell_types_from_statement(statement, types);
            }
        }
        LoweredMatchDecision::Arms { arms } => {
            for arm in arms {
                collect_cell_types_from_decision(arm, types);
            }
        }
        LoweredMatchDecision::Test { then, else_, .. } => {
            collect_cell_types_from_decision(then, types);
            collect_cell_types_from_decision(else_, types);
        }
        LoweredMatchDecision::Bind { type_, then, .. } => {
            types.push(type_.clone());
            collect_cell_types_from_decision(then, types);
        }
        LoweredMatchDecision::Or {
            bindings,
            alternatives,
            then,
            else_,
            ..
        } => {
            for binding in bindings {
                types.push(binding.type_.clone());
            }
            for alternative in alternatives {
                collect_cell_types_from_decision(alternative, types);
            }
            collect_cell_types_from_decision(then, types);
            collect_cell_types_from_decision(else_, types);
        }
        LoweredMatchDecision::Matched | LoweredMatchDecision::Fail | LoweredMatchDecision::End => {}
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
        .push_str("    gust_rt_clone_entry* entry = gust_rt_alloc(&gust_rt_desc_clone_entry, sizeof(gust_rt_clone_entry));\n");
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
        source.push_str("* result = gust_rt_alloc(&");
        push_c_struct_desc_name(source, &struct_.name);
        source.push_str(", sizeof(");
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
        source.push_str("* result = gust_rt_alloc(&");
        push_c_struct_desc_name(source, &struct_.name);
        source.push_str(", sizeof(");
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
            source.push_str("    if (value->gust_capacity > 0) {\n        result->gust_data = gust_rt_alloc(&gust_rt_desc_bytes, sizeof(");
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
            source.push_str("    if (value->gust_capacity > 0) {\n        result->gust_data = gust_rt_alloc(&gust_rt_desc_bytes, value->gust_capacity);\n    }\n");
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

fn push_c_panic_runtime(source: &mut String) {
    source.push_str("typedef struct gust_rt_stack_frame {\n");
    source.push_str("    const char* gust_name;\n");
    source.push_str("    const char* gust_path;\n");
    source.push_str("    size_t gust_line;\n");
    source.push_str("    size_t gust_column;\n");
    source.push_str("} gust_rt_stack_frame;\n\n");
    source.push_str("static gust_rt_stack_frame gust_rt_stack[1024];\n");
    source.push_str("static size_t gust_rt_stack_len = 0;\n\n");
    source.push_str(
        "static void gust_rt_stack_push(const char* name, const char* path, size_t line, size_t column) {\n",
    );
    source.push_str("    if (gust_rt_stack_len < 1024) {\n");
    source.push_str("        gust_rt_stack[gust_rt_stack_len].gust_name = name;\n");
    source.push_str("        gust_rt_stack[gust_rt_stack_len].gust_path = path;\n");
    source.push_str("        gust_rt_stack[gust_rt_stack_len].gust_line = line;\n");
    source.push_str("        gust_rt_stack[gust_rt_stack_len].gust_column = column;\n");
    source.push_str("        gust_rt_stack_len++;\n");
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str(
        "static void gust_rt_stack_update(const char* path, size_t line, size_t column) {\n",
    );
    source.push_str("    if (gust_rt_stack_len > 0) {\n");
    source.push_str("        gust_rt_stack[gust_rt_stack_len - 1].gust_path = path;\n");
    source.push_str("        gust_rt_stack[gust_rt_stack_len - 1].gust_line = line;\n");
    source.push_str("        gust_rt_stack[gust_rt_stack_len - 1].gust_column = column;\n");
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_stack_pop(void) {\n");
    source.push_str("    if (gust_rt_stack_len > 0) {\n");
    source.push_str("        gust_rt_stack_len--;\n");
    source.push_str("    }\n");
    source.push_str("}\n\n");
    source.push_str("static void gust_rt_panic(gust_rt_string message) {\n");
    source.push_str("    fputs(\"panic: \", stderr);\n");
    source.push_str("    fwrite(message.gust_data, 1, message.gust_byte_len, stderr);\n");
    source.push_str("    fputc('\\n', stderr);\n");
    source.push_str("    fputs(\"stack trace:\\n\", stderr);\n");
    source.push_str("    for (size_t index = gust_rt_stack_len; index > 0; index--) {\n");
    source.push_str("        fputs(\"  at \", stderr);\n");
    source.push_str("        fputs(gust_rt_stack[index - 1].gust_name, stderr);\n");
    source.push_str("        fputs(\" (\", stderr);\n");
    source.push_str("        fputs(gust_rt_stack[index - 1].gust_path, stderr);\n");
    source.push_str("        fprintf(stderr, \":%zu:%zu\", gust_rt_stack[index - 1].gust_line, gust_rt_stack[index - 1].gust_column);\n");
    source.push_str("        fputs(\")\\n\", stderr);\n");
    source.push_str("    }\n");
    source.push_str("    exit(101);\n");
    source.push_str("}\n\n");
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
        source.push_str("    unsigned char* data = gust_rt_alloc(&gust_rt_desc_bytes, length);\n");
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
        source.push_str("    unsigned char* data = gust_rt_alloc(&gust_rt_desc_bytes, length);\n");
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
    source.push_str("    unsigned char* data = gust_rt_alloc(&gust_rt_desc_bytes, (size_t)length + 1);\n");
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
