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
        LoweredExprKind::MatchPatternBinding {
            matched_value,
            alternatives,
        } => {
            collect_expr_function_types(matched_value, types);
            for alternative in alternatives {
                collect_expr_function_types(&alternative.value, types);
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
        BasicType::String => "gust_rt_string",
        BasicType::Char => "uint32_t",
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
