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
                if index + 1 < branches.len() {
                    if index > 0 {
                        source.push_str("else ");
                    }
                    source.push_str("if (");
                    push_c_match_condition(source, temp_name, &branch.pattern);
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

fn push_c_match_condition(source: &mut String, temp_name: &str, pattern: &LoweredPattern) {
    match pattern {
        LoweredPattern::Variant { enum_name, variant } => {
            source.push_str(temp_name);
            source.push_str(".gust_tag == ");
            push_c_enum_variant_tag(source, enum_name, variant);
        }
        LoweredPattern::String(value) => {
            source.push_str("gust_rt_string_equal(");
            source.push_str(temp_name);
            source.push_str(", ");
            push_c_string_literal(source, value);
            source.push(')');
        }
        LoweredPattern::Number(value) => {
            source.push_str(temp_name);
            source.push_str(" == ");
            push_c_number_literal(source, value, &LoweredType::Basic(BasicType::I32));
        }
        LoweredPattern::Range {
            start,
            end,
            inclusive,
        } => {
            source.push('(');
            source.push_str(temp_name);
            source.push_str(" >= ");
            push_c_number_literal(source, start, &LoweredType::Basic(BasicType::I32));
            source.push_str(" && ");
            source.push_str(temp_name);
            if *inclusive {
                source.push_str(" <= ");
            } else {
                source.push_str(" < ");
            }
            push_c_number_literal(source, end, &LoweredType::Basic(BasicType::I32));
            source.push(')');
        }
        LoweredPattern::Wildcard => source.push_str("true"),
    }
}

fn push_c_indent(source: &mut String, indent: usize) {
    for _ in 0..indent {
        source.push_str("    ");
    }
}

