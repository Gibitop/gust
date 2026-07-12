fn push_c_statement(
    source: &mut String,
    statement: &LoweredStatement,
    indent: usize,
    structs: &[LoweredStruct],
) {
    match statement {
        LoweredStatement::Local { name, value } => {
            push_c_indent(source, indent);
            push_c_type(source, &value.type_);
            source.push(' ');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::LocalCell { name, value } => {
            push_c_indent(source, indent);
            push_c_type(source, &value.type_);
            source.push_str("* ");
            push_c_local_name(source, name);
            source.push_str(" = gust_rt_alloc(sizeof(");
            push_c_type(source, &value.type_);
            source.push_str("));\n");
            push_c_indent(source, indent);
            source.push('*');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::Assignment { target, value } => {
            push_c_indent(source, indent);
            push_c_value(source, target, structs);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::Println(value) => {
            push_c_indent(source, indent);
            source.push_str("gust_rt_io_println(");
            push_c_value(source, value, structs);
            source.push_str(");\n");
        }
        LoweredStatement::Expr(value) => {
            push_c_indent(source, indent);
            push_c_value(source, value, structs);
            source.push_str(";\n");
        }
        LoweredStatement::Return(value) => {
            push_c_indent(source, indent);
            source.push_str("return");

            if let Some(value) = value {
                source.push(' ');
                push_c_value(source, value, structs);
            }

            source.push_str(";\n");
        }
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            push_c_indent(source, indent);
            source.push_str("if (");
            push_c_value(source, condition, structs);
            source.push_str(") {\n");

            for statement in then_branch {
                push_c_statement(source, statement, indent + 1, structs);
            }

            push_c_indent(source, indent);
            source.push('}');

            if let Some(else_branch) = else_branch {
                source.push_str(" else {\n");

                for statement in else_branch {
                    push_c_statement(source, statement, indent + 1, structs);
                }

                push_c_indent(source, indent);
                source.push('}');
            }

            source.push('\n');
        }
        LoweredStatement::While { condition, body } => {
            push_c_indent(source, indent);
            source.push_str("while (");
            push_c_value(source, condition, structs);
            source.push_str(") {\n");

            for statement in body {
                push_c_statement(source, statement, indent + 1, structs);
            }

            push_c_indent(source, indent);
            source.push_str("}\n");
        }
        LoweredStatement::Break => {
            push_c_indent(source, indent);
            source.push_str("break;\n");
        }
        LoweredStatement::Continue => {
            push_c_indent(source, indent);
            source.push_str("continue;\n");
        }
        LoweredStatement::Match {
            value,
            temp_name,
            branches,
        } => {
            push_c_indent(source, indent);
            source.push_str("{\n");
            push_c_indent(source, indent + 1);
            push_c_type(source, &value.type_);
            source.push(' ');
            source.push_str(temp_name);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");

            for (index, branch) in branches.iter().enumerate() {
                push_c_indent(source, indent + 1);
                if branch.guard.is_some() || !lowered_pattern_is_unconditional(&branch.pattern) {
                    if index > 0 {
                        source.push_str("else ");
                    }
                    source.push_str("if (");
                    push_c_match_branch_condition(
                        source,
                        temp_name,
                        &value.type_,
                        &branch.pattern,
                        branch.guard.as_ref(),
                        structs,
                    );
                    source.push_str(") {\n");
                } else {
                    if index > 0 {
                        source.push_str("else ");
                    }
                    source.push_str("{\n");
                }

                for statement in &branch.statements {
                    push_c_statement(source, statement, indent + 2, structs);
                }

                push_c_indent(source, indent + 1);
                source.push_str("}\n");
            }

            push_c_indent(source, indent);
            source.push_str("}\n");
        }
    }
}

fn push_c_match_branch_condition(
    source: &mut String,
    temp_name: &str,
    value_type: &LoweredType,
    pattern: &LoweredPattern,
    guard: Option<&LoweredExpr>,
    structs: &[LoweredStruct],
) {
    let has_pattern_condition = !lowered_pattern_is_unconditional(pattern);
    if has_pattern_condition {
        push_c_match_condition(source, temp_name, value_type, pattern);
    }
    if let Some(guard) = guard {
        if has_pattern_condition {
            source.push_str(" && ");
        }
        push_c_value(source, guard, structs);
    }
}

fn push_c_match_condition(
    source: &mut String,
    temp_name: &str,
    value_type: &LoweredType,
    pattern: &LoweredPattern,
) {
    let matched_value = LoweredExpr {
        type_: value_type.clone(),
        kind: LoweredExprKind::MatchValue(temp_name.to_string()),
    };
    push_c_match_condition_for_value(source, &matched_value, pattern);
}

fn push_c_match_condition_for_value(
    source: &mut String,
    matched_value: &LoweredExpr,
    pattern: &LoweredPattern,
) {
    let mut value = String::new();
    push_c_value(&mut value, matched_value, &[]);
    push_c_match_condition_for_name(source, &value, pattern);
}

fn push_c_match_condition_for_name(source: &mut String, temp_name: &str, pattern: &LoweredPattern) {
    match pattern {
        LoweredPattern::Or(alternatives) => {
            source.push('(');
            for (index, alternative) in alternatives.iter().enumerate() {
                if index > 0 {
                    source.push_str(" || ");
                }
                push_c_match_condition_for_name(source, temp_name, alternative);
            }
            source.push(')');
        }
        LoweredPattern::Variant {
            enum_name,
            variant,
            payload,
        } => {
            source.push('(');
            source.push_str(temp_name);
            source.push_str(".gust_tag == ");
            push_c_enum_variant_tag(source, enum_name, variant);
            if let Some(payload) = payload {
                source.push_str(" && ");
                let mut payload_name = temp_name.to_string();
                payload_name.push_str(".gust_payload.");
                push_c_local_name(&mut payload_name, variant);
                push_c_match_condition_for_name(source, &payload_name, payload);
            }
            source.push(')');
        }
        LoweredPattern::Struct { fields, .. } => {
            if fields.is_empty() {
                source.push('1');
            } else {
                source.push('(');
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        source.push_str(" && ");
                    }
                    let mut field_name = temp_name.to_string();
                    field_name.push_str("->");
                    push_c_local_name(&mut field_name, &field.name);
                    push_c_match_condition_for_name(source, &field_name, &field.pattern);
                }
                source.push(')');
            }
        }
        LoweredPattern::String(value) => {
            source.push_str("gust_rt_string_equal(");
            source.push_str(temp_name);
            source.push_str(", ");
            push_c_string_literal(source, value);
            source.push(')');
        }
        LoweredPattern::Bool(value) => {
            source.push_str(temp_name);
            source.push_str(" == ");
            if *value {
                source.push_str("true");
            } else {
                source.push_str("false");
            }
        }
        LoweredPattern::Number { value, type_ } => {
            source.push_str(temp_name);
            source.push_str(" == ");
            push_c_number_literal(source, value, &LoweredType::Basic(*type_));
        }
        LoweredPattern::Range {
            start,
            end,
            inclusive,
            type_,
        } => {
            source.push('(');
            source.push_str(temp_name);
            source.push_str(" >= ");
            push_c_number_literal(source, start, &LoweredType::Basic(*type_));
            source.push_str(" && ");
            source.push_str(temp_name);
            if *inclusive {
                source.push_str(" <= ");
            } else {
                source.push_str(" < ");
            }
            push_c_number_literal(source, end, &LoweredType::Basic(*type_));
            source.push(')');
        }
        LoweredPattern::Wildcard => source.push('1'),
    }
}

fn lowered_pattern_is_unconditional(pattern: &LoweredPattern) -> bool {
    match pattern {
        LoweredPattern::Wildcard => true,
        LoweredPattern::Or(alternatives) => alternatives
            .iter()
            .any(lowered_pattern_is_unconditional),
        LoweredPattern::Struct { fields, .. } => fields
            .iter()
            .all(|field| lowered_pattern_is_unconditional(&field.pattern)),
        LoweredPattern::Variant { .. }
        | LoweredPattern::String(_)
        | LoweredPattern::Bool(_)
        | LoweredPattern::Number { .. }
        | LoweredPattern::Range { .. } => false,
    }
}

fn push_c_indent(source: &mut String, indent: usize) {
    for _ in 0..indent {
        source.push_str("    ");
    }
}
