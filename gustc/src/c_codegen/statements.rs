fn push_c_statement(
    source: &mut String,
    statement: &LoweredStatement,
    indent: usize,
    structs: &[LoweredStruct],
    uses_panic: bool,
    uses_gc: bool,
    loop_roots: Option<&str>,
) {
    match statement {
        LoweredStatement::Local { name, value } => {
            push_c_indent(source, indent);
            push_c_type(source, &value.type_);
            source.push(' ');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value_with_panic(source, value, structs, uses_panic);
            source.push_str(";\n");
            if uses_gc {
                push_c_root_for_local(source, name, &value.type_, indent);
                push_c_safepoint(source, indent);
            }
        }
        LoweredStatement::LocalCell { name, value } => {
            push_c_indent(source, indent);
            push_c_type(source, &value.type_);
            source.push_str("* ");
            push_c_local_name(source, name);
            source.push_str(" = gust_rt_alloc(&");
            push_c_cell_desc_name(source, &value.type_);
            source.push_str(", sizeof(");
            push_c_type(source, &value.type_);
            source.push_str("));\n");
            push_c_indent(source, indent);
            source.push('*');
            push_c_local_name(source, name);
            source.push_str(" = ");
            push_c_value_with_panic(source, value, structs, uses_panic);
            source.push_str(";\n");
            if uses_gc {
                push_c_heap_root_for_local(source, name, indent);
                push_c_safepoint(source, indent);
            }
        }
        LoweredStatement::Assignment { target, value } => {
            push_c_indent(source, indent);
            push_c_value_with_panic(source, target, structs, uses_panic);
            source.push_str(" = ");
            push_c_value_with_panic(source, value, structs, uses_panic);
            source.push_str(";\n");
            if uses_gc {
                push_c_safepoint(source, indent);
            }
        }
        LoweredStatement::Println(value) => {
            push_c_indent(source, indent);
            source.push_str("gust_rt_io_println(");
            push_c_value_with_panic(source, value, structs, uses_panic);
            source.push_str(");\n");
            if uses_gc {
                push_c_safepoint(source, indent);
            }
        }
        LoweredStatement::Panic { message, location } => {
            push_c_stack_update(source, location, indent);
            push_c_indent(source, indent);
            source.push_str("gust_rt_panic(");
            push_c_value_with_panic(source, message, structs, uses_panic);
            source.push_str(");\n");
        }
        LoweredStatement::Expr(value) => {
            push_c_indent(source, indent);
            push_c_value_with_panic(source, value, structs, uses_panic);
            source.push_str(";\n");
            if uses_gc {
                push_c_safepoint(source, indent);
            }
        }
        LoweredStatement::Return(value) => {
            push_c_indent(source, indent);
            if let Some(value) = value {
                if uses_panic || uses_gc {
                    push_c_type(source, &value.type_);
                    source.push_str(" gust_rt_return_value = ");
                    push_c_value_with_panic(source, value, structs, uses_panic);
                    source.push_str(";\n");
                    if uses_gc {
                        push_c_indent(source, indent);
                        source.push_str("gust_rt_roots_pop_to(gust_rt_function_roots);\n");
                    }
                    if uses_panic {
                        push_c_indent(source, indent);
                        source.push_str("gust_rt_stack_pop();\n");
                    }
                    push_c_indent(source, indent);
                    source.push_str("return gust_rt_return_value;\n");
                } else {
                    source.push_str("return ");
                    push_c_value_with_panic(source, value, structs, uses_panic);
                    source.push_str(";\n");
                }
            } else {
                if uses_gc {
                    source.push_str("gust_rt_roots_pop_to(gust_rt_function_roots);\n");
                    push_c_indent(source, indent);
                }
                if uses_panic {
                    source.push_str("gust_rt_stack_pop();\n");
                    push_c_indent(source, indent);
                }
                source.push_str("return;\n");
            }
        }
        LoweredStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            push_c_indent(source, indent);
            source.push_str("if (");
            push_c_value_with_panic(source, condition, structs, uses_panic);
            source.push_str(") {\n");
            let then_roots = format!("gust_rt_scope_roots_{indent}_then");
            if uses_gc {
                push_c_scope_base(source, &then_roots, indent + 1);
            }

            for statement in then_branch {
                push_c_statement(
                    source,
                    statement,
                    indent + 1,
                    structs,
                    uses_panic,
                    uses_gc,
                    loop_roots,
                );
            }

            if uses_gc {
                push_c_pop_roots(source, &then_roots, indent + 1);
            }
            push_c_indent(source, indent);
            source.push('}');

            if let Some(else_branch) = else_branch {
                source.push_str(" else {\n");
                let else_roots = format!("gust_rt_scope_roots_{indent}_else");
                if uses_gc {
                    push_c_scope_base(source, &else_roots, indent + 1);
                }

                for statement in else_branch {
                    push_c_statement(
                        source,
                        statement,
                        indent + 1,
                        structs,
                        uses_panic,
                        uses_gc,
                        loop_roots,
                    );
                }

                if uses_gc {
                    push_c_pop_roots(source, &else_roots, indent + 1);
                }
                push_c_indent(source, indent);
                source.push('}');
            }

            source.push('\n');
        }
        LoweredStatement::While { condition, body } => {
            push_c_indent(source, indent);
            source.push_str("while (");
            push_c_value_with_panic(source, condition, structs, uses_panic);
            source.push_str(") {\n");
            let loop_scope_roots = format!("gust_rt_loop_roots_{indent}");
            if uses_gc {
                push_c_scope_base(source, &loop_scope_roots, indent + 1);
            }

            for statement in body {
                push_c_statement(
                    source,
                    statement,
                    indent + 1,
                    structs,
                    uses_panic,
                    uses_gc,
                    Some(&loop_scope_roots),
                );
            }

            if uses_gc {
                push_c_pop_roots(source, &loop_scope_roots, indent + 1);
                push_c_safepoint(source, indent + 1);
            }
            push_c_indent(source, indent);
            source.push_str("}\n");
        }
        LoweredStatement::Break => {
            push_c_indent(source, indent);
            if uses_gc && let Some(loop_roots) = loop_roots {
                source.push_str("gust_rt_roots_pop_to(");
                source.push_str(loop_roots);
                source.push_str(");\n");
                push_c_indent(source, indent);
            }
            source.push_str("break;\n");
        }
        LoweredStatement::Continue => {
            push_c_indent(source, indent);
            if uses_gc && let Some(loop_roots) = loop_roots {
                source.push_str("gust_rt_roots_pop_to(");
                source.push_str(loop_roots);
                source.push_str(");\n");
                push_c_indent(source, indent);
            }
            source.push_str("continue;\n");
        }
        LoweredStatement::Match {
            value,
            temp_name,
            decision,
        } => {
            push_c_indent(source, indent);
            source.push_str("{\n");
            let match_roots = format!("gust_rt_match_roots_{temp_name}");
            if uses_gc {
                push_c_scope_base(source, &match_roots, indent + 1);
            }
            push_c_indent(source, indent + 1);
            push_c_type(source, &value.type_);
            source.push(' ');
            push_c_local_name(source, temp_name);
            source.push_str(" = ");
            push_c_value_with_panic(source, value, structs, uses_panic);
            source.push_str(";\n");
            if uses_gc {
                push_c_root_for_local(source, temp_name, &value.type_, indent + 1);
            }
            push_c_match_decision(
                source,
                decision,
                temp_name,
                None,
                indent + 1,
                structs,
                uses_panic,
                uses_gc,
                loop_roots,
            );
            if uses_gc {
                push_c_pop_roots(source, &match_roots, indent + 1);
                push_c_safepoint(source, indent + 1);
            }
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
    uses_panic: bool,
    uses_gc: bool,
    loop_roots: Option<&str>,
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
        uses_panic,
        uses_gc,
        loop_roots,
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
    uses_panic: bool,
    uses_gc: bool,
    loop_roots: Option<&str>,
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
                    uses_panic,
                    uses_gc,
                    loop_roots,
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
            push_c_match_test(source, subject, test, structs, uses_panic);
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
                uses_panic,
                uses_gc,
                loop_roots,
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
                uses_panic,
                uses_gc,
                loop_roots,
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
            if uses_gc && *declare {
                push_c_root_for_local(source, name, type_, indent);
            }
            push_c_match_decision_with_labels(
                source,
                then,
                match_id,
                result_name,
                fail_label,
                end_label,
                indent,
                structs,
                uses_panic,
                uses_gc,
                loop_roots,
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
                if uses_gc {
                    push_c_root_for_local(source, &binding.name, &binding.type_, indent);
                }
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
                uses_panic,
                uses_gc,
                loop_roots,
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
                uses_panic,
                uses_gc,
                loop_roots,
            );
            push_c_indent(source, indent);
            source.push_str("}\n");
        }
        LoweredMatchDecision::Matched => {}
        LoweredMatchDecision::Body { statements, value } => {
            for statement in statements {
                push_c_statement(
                    source,
                    statement,
                    indent,
                    structs,
                    uses_panic,
                    uses_gc,
                    loop_roots,
                );
            }
            if let (Some(result_name), Some(value)) = (result_name, value) {
                push_c_indent(source, indent);
                push_c_local_name(source, result_name);
                source.push_str(" = ");
                push_c_value_with_panic(source, value, structs, uses_panic);
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
            push_c_match_test(source, subject, test, structs, false);
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
    uses_panic: bool,
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
            push_c_value_with_panic(source, guard, structs, uses_panic);
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

fn push_c_scope_base(source: &mut String, name: &str, indent: usize) {
    push_c_indent(source, indent);
    source.push_str("gust_rt_root_slot* ");
    source.push_str(name);
    source.push_str(" = gust_rt_roots;\n");
}

fn push_c_pop_roots(source: &mut String, name: &str, indent: usize) {
    push_c_indent(source, indent);
    source.push_str("gust_rt_roots_pop_to(");
    source.push_str(name);
    source.push_str(");\n");
}

fn push_c_safepoint(source: &mut String, indent: usize) {
    push_c_indent(source, indent);
    source.push_str("gust_rt_safepoint();\n");
}

fn push_c_root_for_local(source: &mut String, name: &str, type_: &LoweredType, indent: usize) {
    if matches!(type_, LoweredType::Void) {
        return;
    }

    push_c_indent(source, indent);
    source.push_str("gust_rt_root_slot ");
    push_c_root_name(source, name);
    source.push_str(" = { &");
    push_c_local_name(source, name);
    source.push_str(", ");
    push_c_cell_trace_name(source, type_);
    source.push_str(", NULL };\n");
    push_c_indent(source, indent);
    source.push_str("gust_rt_root_push(&");
    push_c_root_name(source, name);
    source.push_str(");\n");
}

fn push_c_heap_root_for_local(source: &mut String, name: &str, indent: usize) {
    push_c_indent(source, indent);
    source.push_str("gust_rt_root_slot ");
    push_c_root_name(source, name);
    source.push_str(" = { &");
    push_c_local_name(source, name);
    source.push_str(", gust_rt_trace_heap_object_root, NULL };\n");
    push_c_indent(source, indent);
    source.push_str("gust_rt_root_push(&");
    push_c_root_name(source, name);
    source.push_str(");\n");
}
