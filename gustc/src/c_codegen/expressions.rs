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

    for chunk in value.as_bytes()[first_chunk_len..].chunks(CHUNK_DIGITS) {
        source.push_str(" * (unsigned __int128)");
        source.push_str(CHUNK_BASE);
        source.push_str(" + ");
        source.push_str(std::str::from_utf8(chunk).expect("numeric literals are ASCII"));
        source.push_str("ULL)");
    }
}

fn push_c_value(source: &mut String, value: &LoweredExpr, structs: &[LoweredStruct]) {
    match &value.kind {
        LoweredExprKind::Void => {}
        LoweredExprKind::StringLiteral(value) => {
            push_c_string_literal(source, value);
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
        LoweredExprKind::LocalCell(name) => {
            source.push_str("(*");
            push_c_local_name(source, name);
            source.push(')');
        }
        LoweredExprKind::CapturedLocal { env_name, name } => {
            source.push_str("(*");
            push_c_local_name(source, env_name);
            source.push_str("->");
            push_c_local_name(source, name);
            source.push(')');
        }
        LoweredExprKind::PostfixIncrement(target) => {
            source.push('(');
            push_c_value(source, target, structs);
            source.push_str("++)");
        }
        LoweredExprKind::StringConcat(left, right) => {
            source.push_str("gust_rt_string_concat(");
            push_c_value(source, left, structs);
            source.push_str(", ");
            push_c_value(source, right, structs);
            source.push(')');
        }
        LoweredExprKind::Not(operand) => {
            source.push_str("(!");
            push_c_value(source, operand, structs);
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
            push_c_value(source, operand, structs);
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
                push_c_value(source, left, structs);
                source.push_str(", ");
                push_c_value(source, right, structs);
                source.push(')');
                return;
            }

            source.push('(');
            push_c_value(source, left, structs);
            source.push(' ');
            source.push_str(op.symbol());
            source.push(' ');
            push_c_value(source, right, structs);
            source.push(')');
        }
        LoweredExprKind::Logical { left, op, right } => {
            source.push('(');
            push_c_value(source, left, structs);
            source.push(' ');
            source.push_str(op.symbol());
            source.push(' ');
            push_c_value(source, right, structs);
            source.push(')');
        }
        LoweredExprKind::Comparison { left, op, right } => {
            if left.type_ == LoweredType::Basic(BasicType::String) {
                if *op == BinaryOp::NotEqual {
                    source.push('!');
                }

                source.push_str("gust_rt_string_equal(");
                push_c_value(source, left, structs);
                source.push_str(", ");
                push_c_value(source, right, structs);
                source.push(')');
            } else {
                source.push('(');
                push_c_value(source, left, structs);
                source.push(' ');
                source.push_str(op.symbol());
                source.push(' ');
                push_c_value(source, right, structs);
                source.push(')');
            }
        }
        LoweredExprKind::StructLiteral { name, fields } => {
            push_c_struct_new_name(source, name);
            source.push('(');
            let struct_ = structs
                .iter()
                .find(|struct_| struct_.name == *name)
                .expect("lowered struct literal must reference a known struct");

            for (index, field) in struct_.fields.iter().enumerate() {
                if index > 0 {
                    source.push_str(", ");
                }

                let value = fields
                    .iter()
                    .find(|value| value.name == field.name)
                    .expect("lowered struct literal must contain every declared field");
                push_c_value(source, &value.value, structs);
            }

            source.push(')');
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
                push_c_value(source, payload, structs);
            }

            source.push_str(" }");
        }
        LoweredExprKind::Match {
            value: matched_value,
            temp_name,
            decision,
        } => {
            let result_name = format!("{temp_name}_result");

            source.push_str("({\n    ");
            push_c_type(source, &matched_value.type_);
            source.push(' ');
            push_c_local_name(source, temp_name);
            source.push_str(" = ");
            push_c_value(source, matched_value, structs);
            source.push_str(";\n    ");
            push_c_type(source, &value.type_);
            source.push(' ');
            push_c_local_name(source, &result_name);
            source.push_str(";\n");

            push_c_match_decision(source, decision, temp_name, Some(&result_name), 1, structs);

            source.push_str("    ");
            push_c_local_name(source, &result_name);
            source.push_str(";\n})");
        }
        LoweredExprKind::FieldAccess { object, field } => {
            push_c_value(source, object, structs);
            if object.type_ == LoweredType::Basic(BasicType::String) {
                source.push('.');
            } else {
                source.push_str("->");
            }
            push_c_local_name(source, field);
        }
        LoweredExprKind::Clone(object) => {
            let LoweredType::Struct(name) = &object.type_ else {
                unreachable!("only struct values use lowered clone expressions")
            };
            push_c_struct_clone_name(source, name);
            source.push('(');
            push_c_value(source, object, structs);
            source.push(')');
        }
        LoweredExprKind::NumberToString(object) => {
            let LoweredType::Basic(type_) = &object.type_ else {
                unreachable!("only basic numeric values use number-to-string expressions")
            };
            source.push_str("gust_rt_");
            source.push_str(type_.name());
            source.push_str("_to_string(");
            push_c_value(source, object, structs);
            source.push(')');
        }
        LoweredExprKind::Call { name, args } => {
            if name == "intrinsic string.len" {
                source.push_str("gust_rt_string_char_len(");
                push_c_value(source, &args[0], structs);
                source.push(')');
                return;
            }
            if let Some(method) = string_builder_method(name) {
                if method == "withCapacity"
                    && let LoweredType::Struct(builder_name) = &value.type_
                    && is_string_builder_name(builder_name)
                {
                    source.push_str("({\n    ");
                    push_c_type(source, &value.type_);
                    source.push_str(" gust_builder = ");
                    push_c_struct_new_name(source, builder_name);
                    source.push_str("();\n    gust_builder->gust_capacity = ");
                    push_c_value(source, &args[0], structs);
                    source.push_str(";\n    if (gust_builder->gust_capacity > 0) {\n        gust_builder->gust_data = gust_rt_alloc(gust_builder->gust_capacity);\n    }\n    gust_builder;\n})");
                    return;
                }
                let builder = args
                    .first()
                    .filter(|builder| matches!(&builder.type_, LoweredType::Struct(name) if is_string_builder_name(name)));
                if let Some(builder) = builder {
                    let LoweredType::Struct(builder_name) = &builder.type_ else {
                        unreachable!("StringBuilder methods require a StringBuilder receiver")
                    };
                    match method {
                        "append" => {
                            source.push_str("gust_rt_string_builder_append_");
                            push_c_struct_name(source, builder_name);
                            source.push('(');
                            push_c_value(source, builder, structs);
                            source.push_str(", ");
                            push_c_value(source, &args[1], structs);
                            source.push(')');
                            return;
                        }
                        "build" => {
                            source.push_str("gust_rt_string_builder_build_");
                            push_c_struct_name(source, builder_name);
                            source.push('(');
                            push_c_value(source, builder, structs);
                            source.push(')');
                            return;
                        }
                        _ => unreachable!("only instance StringBuilder methods reach this branch"),
                    }
                }
            }
            let raw_element = raw_buffer_element_type(structs, &value.type_).or_else(|| {
                args.first()
                    .and_then(|arg| raw_buffer_element_type(structs, &arg.type_))
            });
            if let (Some(method), Some(element)) = (raw_buffer_method(name), raw_element) {
                match method {
                    "withCapacity" => {
                        source.push_str("({\n    ");
                        push_c_type(source, &value.type_);
                        source.push_str(" gust_buffer = gust_rt_alloc(sizeof(*gust_buffer));\n");
                        source.push_str("    memset(gust_buffer, 0, sizeof(*gust_buffer));\n");
                        source.push_str("    gust_buffer->gust_capacity = ");
                        push_c_value(source, &args[0], structs);
                        source.push_str(";\n    if (gust_buffer->gust_capacity > 0) {\n        gust_buffer->gust_data = gust_rt_alloc(sizeof(");
                        push_c_type(source, element);
                        source.push_str(
                            ") * gust_buffer->gust_capacity);\n    }\n    gust_buffer;\n})",
                        );
                        return;
                    }
                    "capacity" => {
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_capacity");
                        return;
                    }
                    "read" => {
                        let LoweredType::Enum(option) = &value.type_ else {
                            unreachable!("raw buffer reads return Option values")
                        };
                        source.push('(');
                        push_c_enum_name(source, option);
                        source.push_str("){ .gust_tag = ");
                        push_c_enum_variant_tag(source, option, "Some");
                        source.push_str(", .gust_payload.");
                        push_c_local_name(source, "Some");
                        source.push_str(" = ((");
                        push_c_type(source, element);
                        source.push_str("*)");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_data)[");
                        push_c_value(source, &args[1], structs);
                        source.push_str("] }");
                        return;
                    }
                    "write" => {
                        source.push_str("((");
                        push_c_type(source, element);
                        source.push_str("*)");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_data)[");
                        push_c_value(source, &args[1], structs);
                        source.push_str("] = ");
                        push_c_value(source, &args[2], structs);
                        source.push_str(", ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length = ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length > ");
                        push_c_value(source, &args[1], structs);
                        source.push_str(" ? ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length : ");
                        push_c_value(source, &args[1], structs);
                        source.push_str(" + 1");
                        return;
                    }
                    "clear" => {
                        source.push_str("({ memset(&( (");
                        push_c_type(source, element);
                        source.push_str("*)");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_data)[");
                        push_c_value(source, &args[1], structs);
                        source.push_str("], 0, sizeof(");
                        push_c_type(source, element);
                        source.push_str(")); if (");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length == ");
                        push_c_value(source, &args[1], structs);
                        source.push_str(" + 1) { ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("->gust_length = ");
                        push_c_value(source, &args[1], structs);
                        source.push_str("; } })");
                        return;
                    }
                    "grow" => {
                        source.push_str("({ ");
                        push_c_type(source, &args[0].type_);
                        source.push_str(" gust_buffer = ");
                        push_c_value(source, &args[0], structs);
                        source.push_str("; size_t gust_capacity = ");
                        push_c_value(source, &args[1], structs);
                        source.push_str("; void* gust_data = gust_rt_alloc(sizeof(");
                        push_c_type(source, element);
                        source.push_str(") * gust_capacity); if (gust_buffer->gust_length > 0) { memcpy(gust_data, gust_buffer->gust_data, sizeof(");
                        push_c_type(source, element);
                        source.push_str(") * gust_buffer->gust_length); } gust_buffer->gust_data = gust_data; gust_buffer->gust_capacity = gust_capacity; })");
                        return;
                    }
                    _ => unreachable!("raw buffer methods are exhaustive"),
                }
            }
            push_c_function_name(source, name);
            source.push('(');

            for (index, arg) in args.iter().enumerate() {
                if index > 0 {
                    source.push_str(", ");
                }

                push_c_value(source, arg, structs);
            }

            source.push(')');
        }
        LoweredExprKind::CollectionLiteral {
            constructor,
            add,
            items,
        } => {
            source.push_str("({\n    ");
            push_c_type(source, &value.type_);
            source.push_str(" gust_collection = ");
            push_c_function_name(source, constructor);
            source.push('(');
            source.push_str(&items.len().to_string());
            source.push_str(");\n");
            for item in items {
                source.push_str("    ");
                push_c_function_name(source, add);
                source.push_str("(gust_collection, ");
                push_c_value(source, item, structs);
                source.push_str(");\n");
            }
            source.push_str("    gust_collection;\n})");
        }
        LoweredExprKind::TraitObject {
            trait_name,
            self_type,
            value,
        } => match self_type {
            LoweredType::Struct(type_name) => {
                source.push('(');
                push_c_trait_name(source, trait_name);
                source.push_str("){ .gust_self = ");
                push_c_value(source, value, structs);
                source.push_str(", .gust_vtable = &");
                push_c_trait_impl_vtable_name(source, trait_name, type_name);
                source.push_str(" }");
            }
            LoweredType::Enum(type_name) => {
                source.push_str("({\n    ");
                push_c_enum_name(source, type_name);
                source.push_str("* gust_trait_self = gust_rt_alloc(sizeof(");
                push_c_enum_name(source, type_name);
                source.push_str("));\n    *gust_trait_self = ");
                push_c_value(source, value, structs);
                source.push_str(";\n    (");
                push_c_trait_name(source, trait_name);
                source.push_str("){ .gust_self = gust_trait_self, .gust_vtable = &");
                push_c_trait_impl_vtable_name(source, trait_name, type_name);
                source.push_str(" };\n})");
            }
            _ => unreachable!("only struct and enum values can be emitted as trait objects"),
        },
        LoweredExprKind::DynamicCall {
            object,
            method,
            args,
        } => {
            let LoweredType::Trait(trait_name) = &object.type_ else {
                unreachable!("dynamic calls require trait-typed receivers")
            };
            source.push_str("({\n    ");
            push_c_trait_name(source, trait_name);
            source.push_str(" gust_trait_value = ");
            push_c_value(source, object, structs);
            source.push_str(";\n    ");
            if value.type_ != LoweredType::Void {
                push_c_type(source, &value.type_);
                source.push_str(" gust_trait_result = ");
            }
            source.push_str("gust_trait_value.gust_vtable->");
            push_c_trait_method_field_name(source, method);
            source.push_str("(gust_trait_value.gust_self");
            for arg in args {
                source.push_str(", ");
                push_c_value(source, arg, structs);
            }
            source.push_str(");\n");
            if value.type_ != LoweredType::Void {
                source.push_str("    gust_trait_result;\n");
            }
            source.push_str("})");
        }
        LoweredExprKind::Closure { name, captures } => {
            let LoweredType::Function { .. } = &value.type_ else {
                unreachable!("closure expressions must have function type")
            };
            if captures.is_empty() {
                source.push('(');
                push_c_type(source, &value.type_);
                source.push_str("){ .gust_env = NULL, .gust_call = ");
                push_c_function_name(source, name);
                source.push_str(" }");
            } else {
                let env_type = closure_env_type_name(name);
                source.push_str("({\n    ");
                source.push_str(&env_type);
                source.push_str("* gust_env = gust_rt_alloc(sizeof(");
                source.push_str(&env_type);
                source.push_str("));\n");
                for capture in captures {
                    source.push_str("    gust_env->");
                    push_c_local_name(source, &capture.name);
                    source.push_str(" = ");
                    push_c_local_name(source, &capture.name);
                    source.push_str(";\n");
                }
                source.push_str("    (");
                push_c_type(source, &value.type_);
                source.push_str("){ .gust_env = gust_env, .gust_call = ");
                push_c_function_name(source, name);
                source.push_str(" };\n})");
            }
        }
        LoweredExprKind::IndirectCall { callee, args } => {
            push_c_value(source, callee, structs);
            source.push_str(".gust_call(");
            push_c_value(source, callee, structs);
            source.push_str(".gust_env");
            for arg in args {
                source.push_str(", ");
                push_c_value(source, arg, structs);
            }
            source.push(')');
        }
    }
}

fn push_c_string_literal(source: &mut String, value: &str) {
    source.push_str("(gust_rt_string){ .gust_data = (const unsigned char*)\"");
    push_c_string_value(source, value);
    source.push_str("\", .gust_byte_len = ");
    source.push_str(&value.len().to_string());
    source.push_str(" }");
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
