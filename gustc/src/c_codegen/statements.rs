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
            decision,
        } => {
            push_c_indent(source, indent);
            source.push_str("{\n");
            push_c_indent(source, indent + 1);
            push_c_type(source, &value.type_);
            source.push(' ');
            push_c_local_name(source, temp_name);
            source.push_str(" = ");
            push_c_value(source, value, structs);
            source.push_str(";\n");
            push_c_match_decision(source, decision, temp_name, None, indent + 1, structs);
            push_c_indent(source, indent);
            source.push_str("}\n");
        }
    }
}

fn push_c_match_decision(
    source: &mut String,
    decision: &LoweredMatchDecision,
    match_id: &str,
    result_name: Option<&str>,
    indent: usize,
    structs: &[LoweredStruct],
) {
    push_c_match_decision_with_labels(
        source,
        decision,
        match_id,
        result_name,
        None,
        None,
        indent,
        structs,
    );
}

fn push_c_match_decision_with_labels(
    source: &mut String,
    decision: &LoweredMatchDecision,
    match_id: &str,
    result_name: Option<&str>,
    fail_label: Option<&str>,
    end_label: Option<&str>,
    indent: usize,
    structs: &[LoweredStruct],
) {
    match decision {
        LoweredMatchDecision::Arms { arms } => {
            let end = format!("gust_match_end_{match_id}");
            for (index, arm) in arms.iter().enumerate() {
                let fail = format!("gust_match_arm_{match_id}_{}", index + 1);
                push_c_match_decision_with_labels(
                    source,
                    arm,
                    match_id,
                    result_name,
                    Some(&fail),
                    Some(&end),
                    indent,
                    structs,
                );
                push_c_indent(source, indent);
                source.push_str(&fail);
                source.push_str(":;\n");
            }
            push_c_indent(source, indent);
            source.push_str(&end);
            source.push_str(":;\n");
        }
        LoweredMatchDecision::Test {
            subject,
            test,
            then,
            else_,
        } => {
            push_c_indent(source, indent);
            source.push_str("if (");
            push_c_match_test(source, subject, test, structs);
            source.push_str(") {\n");
            push_c_match_decision_with_labels(
                source,
                then,
                match_id,
                result_name,
                fail_label,
                end_label,
                indent + 1,
                structs,
            );
            push_c_indent(source, indent);
            source.push_str("} else {\n");
            push_c_match_decision_with_labels(
                source,
                else_,
                match_id,
                result_name,
                fail_label,
                end_label,
                indent + 1,
                structs,
            );
            push_c_indent(source, indent);
            source.push_str("}\n");
        }
        LoweredMatchDecision::Bind {
            name,
            type_,
            source: bind_source,
            declare,
            then,
        } => {
            push_c_indent(source, indent);
            if *declare {
                push_c_type(source, type_);
                source.push(' ');
            }
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_match_bind_source(source, bind_source, type_);
            source.push_str(";\n");
            push_c_match_decision_with_labels(
                source,
                then,
                match_id,
                result_name,
                fail_label,
                end_label,
                indent,
                structs,
            );
        }
        LoweredMatchDecision::Or {
            bindings,
            alternatives,
            then,
            else_,
        } => {
            let matched_flag = format!("internal_match_or_{match_id}_{indent}");
            for binding in bindings {
                push_c_indent(source, indent);
                push_c_type(source, &binding.type_);
                source.push(' ');
                push_c_local_name(source, &binding.name);
                source.push_str(";\n");
            }
            push_c_indent(source, indent);
            source.push_str("bool ");
            push_c_local_name(source, &matched_flag);
            source.push_str(" = false;\n");
            for alternative in alternatives {
                push_c_indent(source, indent);
                source.push_str("if (!");
                push_c_local_name(source, &matched_flag);
                source.push_str(") {\n");
                push_c_match_or_alternative(
                    source,
                    alternative,
                    &matched_flag,
                    indent + 1,
                    structs,
                );
                push_c_indent(source, indent);
                source.push_str("}\n");
            }
            push_c_indent(source, indent);
            source.push_str("if (");
            push_c_local_name(source, &matched_flag);
            source.push_str(") {\n");
            push_c_match_decision_with_labels(
                source,
                then,
                match_id,
                result_name,
                fail_label,
                end_label,
                indent + 1,
                structs,
            );
            push_c_indent(source, indent);
            source.push_str("} else {\n");
            push_c_match_decision_with_labels(
                source,
                else_,
                match_id,
                result_name,
                fail_label,
                end_label,
                indent + 1,
                structs,
            );
            push_c_indent(source, indent);
            source.push_str("}\n");
        }
        LoweredMatchDecision::Matched => {}
        LoweredMatchDecision::Body { statements, value } => {
            for statement in statements {
                push_c_statement(source, statement, indent, structs);
            }
            if let (Some(result_name), Some(value)) = (result_name, value) {
                push_c_indent(source, indent);
                push_c_local_name(source, result_name);
                source.push_str(" = ");
                push_c_value(source, value, structs);
                source.push_str(";\n");
            }
            if let Some(end_label) = end_label {
                push_c_indent(source, indent);
                source.push_str("goto ");
                source.push_str(end_label);
                source.push_str(";\n");
            }
        }
        LoweredMatchDecision::Fail => {
            if let Some(fail_label) = fail_label {
                push_c_indent(source, indent);
                source.push_str("goto ");
                source.push_str(fail_label);
                source.push_str(";\n");
            }
        }
        LoweredMatchDecision::End => {}
    }
}

fn push_c_match_or_alternative(
    source: &mut String,
    decision: &LoweredMatchDecision,
    matched_flag: &str,
    indent: usize,
    structs: &[LoweredStruct],
) {
    match decision {
        LoweredMatchDecision::Test {
            subject,
            test,
            then,
            else_,
        } => {
            push_c_indent(source, indent);
            source.push_str("if (");
            push_c_match_test(source, subject, test, structs);
            source.push_str(") {\n");
            push_c_match_or_alternative(source, then, matched_flag, indent + 1, structs);
            push_c_indent(source, indent);
            source.push_str("} else {\n");
            push_c_match_or_alternative(source, else_, matched_flag, indent + 1, structs);
            push_c_indent(source, indent);
            source.push_str("}\n");
        }
        LoweredMatchDecision::Bind {
            name,
            type_,
            source: bind_source,
            declare,
            then,
        } => {
            push_c_indent(source, indent);
            if *declare {
                push_c_type(source, type_);
                source.push(' ');
            }
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_match_bind_source(source, bind_source, type_);
            source.push_str(";\n");
            push_c_match_or_alternative(source, then, matched_flag, indent, structs);
        }
        LoweredMatchDecision::Matched => {
            push_c_indent(source, indent);
            push_c_local_name(source, matched_flag);
            source.push_str(" = true;\n");
        }
        LoweredMatchDecision::Or { .. }
        | LoweredMatchDecision::Arms { .. }
        | LoweredMatchDecision::Body { .. }
        | LoweredMatchDecision::Fail
        | LoweredMatchDecision::End => {}
    }
}

fn push_c_match_test(
    source: &mut String,
    subject: &str,
    test: &LoweredMatchTest,
    structs: &[LoweredStruct],
) {
    match test {
        LoweredMatchTest::EnumTag { enum_name, variant } => {
            push_c_local_name(source, subject);
            source.push_str(".gust_tag == ");
            push_c_enum_variant_tag(source, enum_name, variant);
        }
        LoweredMatchTest::StringEq(value) => {
            source.push_str("gust_rt_string_equal(");
            push_c_local_name(source, subject);
            source.push_str(", ");
            push_c_string_literal(source, value);
            source.push(')');
        }
        LoweredMatchTest::BoolEq(value) => {
            push_c_local_name(source, subject);
            source.push_str(" == ");
            if *value {
                source.push_str("true");
            } else {
                source.push_str("false");
            }
        }
        LoweredMatchTest::NumberEq { value, type_ } => {
            push_c_local_name(source, subject);
            source.push_str(" == ");
            push_c_number_literal(source, value, &LoweredType::Basic(*type_));
        }
        LoweredMatchTest::Range {
            start,
            end,
            inclusive,
            type_,
        } => {
            source.push('(');
            push_c_local_name(source, subject);
            source.push_str(" >= ");
            push_c_number_literal(source, start, &LoweredType::Basic(*type_));
            source.push_str(" && ");
            push_c_local_name(source, subject);
            if *inclusive {
                source.push_str(" <= ");
            } else {
                source.push_str(" < ");
            }
            push_c_number_literal(source, end, &LoweredType::Basic(*type_));
            source.push(')');
        }
        LoweredMatchTest::Guard(guard) => {
            push_c_value(source, guard, structs);
        }
    }
}

fn push_c_match_bind_source(
    source: &mut String,
    bind_source: &LoweredMatchBindSource,
    _type_: &LoweredType,
) {
    match bind_source {
        LoweredMatchBindSource::EnumPayload { subject, variant } => {
            push_c_local_name(source, subject);
            source.push_str(".gust_payload.");
            push_c_local_name(source, variant);
        }
        LoweredMatchBindSource::StructField { subject, field } => {
            push_c_local_name(source, subject);
            source.push_str("->");
            push_c_local_name(source, field);
        }
        LoweredMatchBindSource::Subject(name) => push_c_local_name(source, name),
    }
}

fn push_c_indent(source: &mut String, indent: usize) {
    for _ in 0..indent {
        source.push_str("    ");
    }
}
