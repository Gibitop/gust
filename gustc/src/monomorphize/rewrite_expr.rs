impl Monomorphizer {
    fn rewrite_expr(&mut self, expr: &mut Expr, substitutions: &HashMap<String, TypeRef>) {
        if matches!(expr.kind, ExprKind::Array(_)) {
            let expected = self.expected_expr_types.remove(&expr.span);
            self.rewrite_collection_literal(expr, substitutions, expected);
            return;
        }

        let generic_function_call = match &expr.kind {
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Identifier(name) if self.function_templates.contains_key(name) => {
                    Some((name.clone(), None))
                }
                ExprKind::GenericType { name, args }
                    if self.function_templates.contains_key(name) =>
                {
                    Some((name.clone(), Some(args.clone())))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((function_name, explicit_args)) = generic_function_call {
            let expected_return = self.expected_expr_types.remove(&expr.span);
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic function call was matched above")
            };
            let mut args_rewritten = false;
            let type_args = if let Some(mut type_args) = explicit_args {
                for type_arg in &mut type_args {
                    self.rewrite_type(type_arg, substitutions);
                }
                self.validate_function_type_arguments(&function_name, &type_args, expr.span)
                    .then_some(type_args)
            } else {
                match self.infer_function_type_arguments(
                    &function_name,
                    args,
                    expected_return.as_ref(),
                ) {
                    Ok(mut type_args) => {
                        for type_arg in &mut type_args {
                            self.rewrite_type(type_arg, substitutions);
                        }
                        Some(type_args)
                    }
                    Err(_) => {
                        for arg in args.iter_mut() {
                            self.rewrite_expr(arg, substitutions);
                        }
                        args_rewritten = true;
                        match self.infer_function_type_arguments(
                            &function_name,
                            args,
                            expected_return.as_ref(),
                        ) {
                            Ok(mut type_args) => {
                                for type_arg in &mut type_args {
                                    self.rewrite_type(type_arg, substitutions);
                                }
                                Some(type_args)
                            }
                            Err(message) => {
                                self.diagnostics.push(Diagnostic::error(expr.span, message));
                                None
                            }
                        }
                    }
                }
            };
            if let Some(type_args) = type_args {
                self.apply_generic_function_argument_contexts(
                    &function_name,
                    &type_args,
                    args,
                    substitutions,
                );
                if !args_rewritten {
                    for arg in args.iter_mut() {
                        self.rewrite_expr(arg, substitutions);
                    }
                }
                self.specialize_function(&function_name, &type_args);
                callee.kind = ExprKind::Identifier(specialized_name(&function_name, &type_args));
            } else if !args_rewritten {
                for arg in args.iter_mut() {
                    self.rewrite_expr(arg, substitutions);
                }
            }
            return;
        }

        let generic_method_call = match &expr.kind {
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::GenericMember { object, name, args } => self
                    .infer_type_expression_ref(object)
                    .filter(|receiver| self.method_template(receiver, name, true).is_some())
                    .map(|receiver| {
                        (
                            object.clone(),
                            receiver,
                            name.clone(),
                            true,
                            Some(args.clone()),
                        )
                    })
                    .or_else(|| {
                        self.infer_expr_type(object).map(|receiver| {
                            self.method_template(&receiver, name, false)
                                .map(|_| {
                                    (
                                        object.clone(),
                                        receiver,
                                        name.clone(),
                                        false,
                                        Some(args.clone()),
                                    )
                                })
                        })
                        .flatten()
                    }),
                ExprKind::Member { object, name } => self
                    .infer_type_expression_ref(object)
                    .and_then(|receiver| {
                        self.method_template(&receiver, name, true)
                            .filter(|(_, _, function)| !function.type_params.is_empty())
                            .map(|_| (object.clone(), receiver, name.clone(), true, None))
                    })
                    .or_else(|| {
                        self.infer_expr_type(object)
                            .and_then(|receiver| {
                                self.method_template(&receiver, name, false)
                                    .filter(|(_, _, function)| !function.type_params.is_empty())
                                    .map(|_| receiver)
                            })
                            .map(|receiver| (object.clone(), receiver, name.clone(), false, None))
                    }),
                _ => None,
            },
            _ => None,
        };
        if let Some((_, receiver, method_name, static_, explicit_args)) = generic_method_call {
            let expected_return = self.expected_expr_types.remove(&expr.span);
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic method call was matched above")
            };
            let mut args_rewritten = false;
            let type_args = if let Some(mut type_args) = explicit_args {
                for type_arg in &mut type_args {
                    self.rewrite_type(type_arg, substitutions);
                }
                self.validate_method_type_arguments(
                    &receiver,
                    &method_name,
                    static_,
                    &type_args,
                    expr.span,
                )
                .then_some(type_args)
            } else {
                match self.infer_method_type_arguments(
                    &receiver,
                    &method_name,
                    static_,
                    args,
                    expected_return.as_ref(),
                ) {
                    Ok(mut type_args) => {
                        for type_arg in &mut type_args {
                            self.rewrite_type(type_arg, substitutions);
                        }
                        Some(type_args)
                    }
                    Err(_) => {
                        for arg in args.iter_mut() {
                            self.rewrite_expr(arg, substitutions);
                        }
                        args_rewritten = true;
                        match self.infer_method_type_arguments(
                            &receiver,
                            &method_name,
                            static_,
                            args,
                            expected_return.as_ref(),
                        ) {
                            Ok(mut type_args) => {
                                for type_arg in &mut type_args {
                                    self.rewrite_type(type_arg, substitutions);
                                }
                                Some(type_args)
                            }
                            Err(message) => {
                                self.diagnostics.push(Diagnostic::error(expr.span, message));
                                None
                            }
                        }
                    }
                }
            };
            if let Some(type_args) = type_args {
                self.apply_generic_method_argument_contexts(
                    &receiver,
                    &method_name,
                    static_,
                    &type_args,
                    args,
                    substitutions,
                );
                if !args_rewritten {
                    for arg in args.iter_mut() {
                        self.rewrite_expr(arg, substitutions);
                    }
                }
                if let Some((receiver, _, _)) =
                    self.method_template(&receiver, &method_name, static_)
                {
                    self.specialize_method(&receiver.name, &method_name, static_, &type_args);
                }
                let mut object = match &mut callee.kind {
                    ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
                        (**object).clone()
                    }
                    _ => unreachable!("generic method call requires a member callee"),
                };
                self.rewrite_expr(&mut object, substitutions);
                callee.kind = ExprKind::Member {
                    object: Box::new(object),
                    name: specialized_name(&method_name, &type_args),
                };
            } else if !args_rewritten {
                for arg in args.iter_mut() {
                    self.rewrite_expr(arg, substitutions);
                }
            }
            return;
        }

        let extension_call = match &expr.kind {
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::GenericMember { object, name, args } => self
                    .infer_type_expression_ref(object)
                    .filter(|receiver| self.method_template(receiver, name, true).is_none())
                    .map(|receiver| {
                        (
                            object.clone(),
                            receiver,
                            name.clone(),
                            true,
                            Some(args.clone()),
                        )
                    })
                    .or_else(|| {
                        self.infer_expr_type(object)
                            .filter(|receiver| {
                                self.method_template(receiver, name, false).is_none()
                            })
                            .map(|receiver| {
                                (
                                    object.clone(),
                                    receiver,
                                    name.clone(),
                                    false,
                                    Some(args.clone()),
                                )
                            })
                    }),
                ExprKind::Member { object, name } => self
                    .infer_type_expression_ref(object)
                    .filter(|receiver| self.method_template(receiver, name, true).is_none())
                    .map(|receiver| (object.clone(), receiver, name.clone(), true, None))
                    .or_else(|| {
                        self.infer_expr_type(object)
                            .filter(|receiver| {
                                self.method_template(receiver, name, false).is_none()
                            })
                            .map(|receiver| (object.clone(), receiver, name.clone(), false, None))
                    }),
                _ => None,
            },
            _ => None,
        };
        if let Some((_, receiver, method_name, static_, explicit_args)) = extension_call {
            let expected_return = self.expected_expr_types.get(&expr.span).cloned();
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("extension call requires a call expression")
            };
            let mut args_rewritten = false;
            let mut extension_error = false;
            let mut explicit_args = explicit_args;
            if let Some(type_args) = &mut explicit_args {
                for type_arg in type_args {
                    self.rewrite_type(type_arg, substitutions);
                }
            }
            let resolution = match self.resolve_extension(
                &receiver,
                &method_name,
                static_,
                explicit_args.as_deref(),
                args,
                expected_return.as_ref(),
            ) {
                Ok(resolution) => resolution,
                Err(_) if explicit_args.is_none() => {
                    for arg in args.iter_mut() {
                        self.rewrite_expr(arg, substitutions);
                    }
                    args_rewritten = true;
                    match self.resolve_extension(
                        &receiver,
                        &method_name,
                        static_,
                        None,
                        args,
                        expected_return.as_ref(),
                    ) {
                        Ok(resolution) => resolution,
                        Err(message) => {
                            extension_error = true;
                            self.diagnostics.push(Diagnostic::error(expr.span, message));
                            None
                        }
                    }
                }
                Err(message) => {
                    extension_error = true;
                    self.diagnostics.push(Diagnostic::error(expr.span, message));
                    None
                }
            };

            if let Some(resolution) = resolution {
                self.expected_expr_types.remove(&expr.span);
                self.apply_extension_argument_contexts(&resolution.params, args, substitutions);
                if !args_rewritten {
                    for arg in args.iter_mut() {
                        self.rewrite_expr(arg, substitutions);
                    }
                }
                self.specialize_extension(&resolution, expr.span);
                if let Some(return_type) = resolution.return_type {
                    self.inferred_expr_types.insert(expr.span, return_type);
                }
                let mut object = match &mut callee.kind {
                    ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
                        (**object).clone()
                    }
                    _ => unreachable!("extension call requires a member callee"),
                };
                self.rewrite_expr(&mut object, substitutions);
                let name = if resolution.function_type_args.is_empty() {
                    method_name
                } else {
                    specialized_name(&method_name, &resolution.function_type_args)
                };
                callee.kind = ExprKind::Member {
                    object: Box::new(object),
                    name,
                };
                return;
            } else if extension_error {
                self.expected_expr_types.remove(&expr.span);
                if !args_rewritten {
                    for arg in args.iter_mut() {
                        self.rewrite_expr(arg, substitutions);
                    }
                }
                return;
            } else if !args_rewritten {
                // No visible extension matched this member, so leave the call for the
                // remaining generic trait/static resolution paths.
            }
        }

        let generic_trait_call = match &expr.kind {
            ExprKind::Call { callee, args } => {
                let ExprKind::Member { object, name } = &callee.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                let mut object = (**object).clone();
                if let ExprKind::Identifier(type_param) = &object.kind
                    && let Some(substitution) = substitutions.get(type_param)
                {
                    let mut type_ref = substitution.clone();
                    self.rewrite_type(&mut type_ref, substitutions);
                    object.kind = ExprKind::Identifier(type_ref.name);
                } else {
                    self.rewrite_expr(&mut object, substitutions);
                }
                self.infer_type_expression_ref(&object)
                    .map(|receiver| (object.clone(), receiver, name.clone(), true, args))
                    .or_else(|| {
                        self.infer_expr_type(&object)
                            .map(|receiver| (object.clone(), receiver, name.clone(), false, args))
                    })
            }
            _ => None,
        };
        if let Some((object, receiver, method_name, static_, source_args)) = generic_trait_call
            && !self.has_real_or_extension_method(&receiver, &method_name, static_)
        {
            let mut expected_return = self.expected_expr_types.get(&expr.span).cloned();
            if let Some(expected_return) = &mut expected_return {
                self.rewrite_type(expected_return, substitutions);
            }
            match self.resolve_generic_trait_method(
                &receiver,
                &method_name,
                static_,
                source_args,
                expected_return.as_ref(),
            ) {
                Ok(Some(mut resolution)) => {
                    self.expected_expr_types.remove(&expr.span);
                    self.impl_receiver_types.push(receiver.clone());
                    for type_arg in &mut resolution.trait_args {
                        self.rewrite_type(type_arg, substitutions);
                    }
                    for type_arg in &mut resolution.impl_type_args {
                        self.rewrite_type(type_arg, substitutions);
                    }
                    for binding in &mut resolution.associated_type_bindings {
                        self.rewrite_type(&mut binding.type_ref, substitutions);
                    }
                    self.rewrite_type(&mut resolution.return_type, substitutions);
                    self.record_type_param_bound_checks(
                        format!(
                            "impl `{} for {}`",
                            specialized_name(&resolution.trait_name, &resolution.trait_args),
                            type_name(&receiver)
                        ),
                        &resolution.impl_type_params,
                        &resolution.impl_type_param_bounds,
                        &resolution.impl_type_args,
                        expr.span,
                    );
                    self.specialize_trait(
                        &resolution.trait_name,
                        &resolution.trait_args,
                        &resolution.associated_type_bindings,
                        expr.span,
                    );

                    let ExprKind::Call { callee, args } = &mut expr.kind else {
                        unreachable!("generic trait method call was matched above")
                    };
                    for (param, arg) in resolution.params.iter_mut().zip(args.iter_mut()) {
                        self.rewrite_type(param, substitutions);
                        self.apply_expr_context(arg, param);
                        self.rewrite_expr(arg, substitutions);
                    }
                    callee.kind = ExprKind::Member {
                        object: Box::new(object),
                        name: format!(
                            "{}::{method_name}",
                            specialized_trait_name(
                                &resolution.trait_name,
                                &resolution.trait_args,
                                &resolution.associated_type_bindings,
                            )
                        ),
                    };
                    self.inferred_expr_types
                        .insert(expr.span, resolution.return_type);
                    return;
                }
                Ok(None) => {}
                Err(message) => {
                    self.diagnostics.push(Diagnostic::error(expr.span, message));
                    self.rewrite_expr_children(expr, substitutions);
                    return;
                }
            }
        }

        if let ExprKind::GenericType { name, args } = &mut expr.kind {
            for arg in args.iter_mut() {
                self.rewrite_type(arg, substitutions);
            }
            if self.struct_templates.contains_key(name) {
                self.specialize_struct(name, args, expr.span);
                *name = specialized_name(name, args);
                expr.kind = ExprKind::Identifier(name.clone());
            } else if self.enum_templates.contains_key(name) {
                self.specialize_enum(name, args, expr.span);
                *name = specialized_name(name, args);
                expr.kind = ExprKind::Identifier(name.clone());
            } else if self.concrete_structs.contains(name) {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("struct `{name}` does not accept type arguments"),
                ));
            } else if self.concrete_enums.contains_key(name) {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("enum `{name}` does not accept type arguments"),
                ));
            } else {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown generic type `{name}`"),
                ));
            }
            return;
        }

        let generic_variant_call = match &expr.kind {
            ExprKind::Call { callee, .. } => {
                let ExprKind::Member { object, name } = &callee.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                let ExprKind::Identifier(type_name) = &object.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                (self.enum_templates.contains_key(type_name)
                    && self.lookup_local_type(type_name).is_none())
                .then(|| (type_name.clone(), name.clone()))
            }
            _ => None,
        };
        if let Some((type_name, variant_name)) = generic_variant_call {
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic variant call was matched above")
            };
            for arg in args.iter_mut() {
                self.rewrite_expr(arg, substitutions);
            }
            match self.infer_enum_type_arguments(&type_name, &variant_name, args) {
                Ok(mut type_args) => {
                    for type_arg in &mut type_args {
                        self.rewrite_type(type_arg, substitutions);
                    }
                    self.specialize_enum(&type_name, &type_args, expr.span);
                    let ExprKind::Member { object, .. } = &mut callee.kind else {
                        unreachable!("generic variant call requires a member callee")
                    };
                    object.kind = ExprKind::Identifier(specialized_name(&type_name, &type_args));
                }
                Err(message) => self.diagnostics.push(Diagnostic::error(expr.span, message)),
            }
            return;
        }

        if let ExprKind::Member { object, name } = &expr.kind
            && let ExprKind::Identifier(type_name) = &object.kind
            && self.enum_templates.contains_key(type_name)
            && self.lookup_local_type(type_name).is_none()
        {
            let message = self
                .infer_enum_type_arguments(type_name, name, &[])
                .err()
                .unwrap_or_else(|| {
                    format!(
                        "cannot infer type arguments for generic enum `{type_name}`; write `{type_name}<Type>.{name}` or add a concrete expected type"
                    )
                });
            self.diagnostics.push(Diagnostic::error(expr.span, message));
            return;
        }

        let generic_static_call = match &expr.kind {
            ExprKind::Call { callee, .. } => {
                let ExprKind::Member { object, name } = &callee.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                let ExprKind::Identifier(type_name) = &object.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                ((self.struct_templates.contains_key(type_name)
                    || self.enum_templates.get(type_name).is_some_and(|template| {
                        find_method_member(&template.members, name, true).is_some()
                    }))
                    && self.lookup_local_type(type_name).is_none())
                .then(|| (type_name.clone(), name.clone()))
            }
            _ => None,
        };
        if let Some((type_name, method_name)) = generic_static_call {
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic static call was matched above")
            };
            for arg in args.iter_mut() {
                self.rewrite_expr(arg, substitutions);
            }
            match self.infer_static_type_arguments(&type_name, &method_name, args) {
                Ok(mut type_args) => {
                    for type_arg in &mut type_args {
                        self.rewrite_type(type_arg, substitutions);
                    }
                    if self.struct_templates.contains_key(&type_name) {
                        self.specialize_struct(&type_name, &type_args, expr.span);
                    } else {
                        self.specialize_enum(&type_name, &type_args, expr.span);
                    }
                    let ExprKind::Member { object, .. } = &mut callee.kind else {
                        unreachable!("generic static call requires a member callee")
                    };
                    object.kind = ExprKind::Identifier(specialized_name(&type_name, &type_args));
                }
                Err(message) => self.diagnostics.push(Diagnostic::error(expr.span, message)),
            }
            return;
        }

        self.rewrite_expr_children(expr, substitutions);
    }

    fn rewrite_collection_literal(
        &mut self,
        expr: &mut Expr,
        substitutions: &HashMap<String, TypeRef>,
        expected: Option<TypeRef>,
    ) {
        let ExprKind::Array(items) = &mut expr.kind else {
            unreachable!("collection literal rewriting requires an array expression")
        };

        let has_expected = expected.is_some();
        let collection = if let Some(mut expected) = expected {
            self.rewrite_type(&mut expected, substitutions);
            expected
        } else {
            for item in items.iter_mut() {
                self.rewrite_expr(item, substitutions);
            }

            let Some(element) = items.first().and_then(|item| self.infer_expr_type(item)) else {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    "cannot infer an element type for an empty collection literal; add a collection type annotation",
                ));
                return;
            };

            for item in items.iter().skip(1) {
                if let Some(type_) = self.infer_expr_type(item)
                    && type_name(&self.expanded_type(&type_))
                        != type_name(&self.expanded_type(&element))
                {
                    self.diagnostics.push(Diagnostic::error(
                        item.span,
                        format!(
                            "collection literal element has type `{}`, expected `{}`",
                            type_name(&type_),
                            type_name(&element)
                        ),
                    ));
                }
            }

            let Some(array_list) = self
                .struct_templates
                .keys()
                .find(|name| *name == "ArrayList" || name.ends_with("::ArrayList"))
                .cloned()
            else {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    "collection literals without a target type require an imported `ArrayList`",
                ));
                return;
            };

            let mut collection = TypeRef {
                name: array_list,
                args: vec![element],
                bindings: Vec::new(),
                function: None,
                span: expr.span,
            };
            self.rewrite_type(&mut collection, substitutions);
            collection
        };

        let element_type = self
            .specializations
            .get(&collection.name)
            .and_then(|(_, args)| args.first().cloned());
        if let Some(element_type) = element_type {
            for item in items.iter_mut() {
                self.apply_expr_context(item, &element_type);
                self.rewrite_expr(item, substitutions);
            }
        } else if has_expected {
            for item in items.iter_mut() {
                self.rewrite_expr(item, substitutions);
            }
        }

        expr.kind = ExprKind::CollectionLiteral {
            items: std::mem::take(items),
            collection,
        };
    }

    fn rewrite_expr_children(&mut self, expr: &mut Expr, substitutions: &HashMap<String, TypeRef>) {
        match &mut expr.kind {
            ExprKind::Array(items) => {
                for item in items {
                    self.rewrite_expr(item, substitutions);
                }
            }
            ExprKind::CollectionLiteral { items, collection } => {
                self.rewrite_type(collection, substitutions);
                for item in items {
                    self.rewrite_expr(item, substitutions);
                }
            }
            ExprKind::Call { callee, args } => {
                self.rewrite_expr(callee, substitutions);
                let function_contexts = if let ExprKind::Identifier(name) = &callee.kind {
                    self.function_params.get(name).cloned()
                } else {
                    None
                };
                let payload_context = if let ExprKind::Member { object, name } = &callee.kind
                    && let ExprKind::Identifier(enum_name) = &object.kind
                {
                    self.enum_variant_payload(enum_name, name).flatten()
                } else {
                    None
                };
                if let (Some(mut expected), Some(arg)) = (payload_context, args.first_mut()) {
                    self.rewrite_type(&mut expected, substitutions);
                    self.apply_expr_context(arg, &expected);
                }
                if let Some(contexts) = function_contexts {
                    for (arg, expected) in args.iter_mut().zip(contexts) {
                        let Some(mut expected) = expected else {
                            continue;
                        };
                        self.rewrite_type(&mut expected, substitutions);
                        self.apply_expr_context(arg, &expected);
                    }
                }
                for arg in args {
                    self.rewrite_expr(arg, substitutions);
                }
            }
            ExprKind::Member { object, .. } => self.rewrite_expr(object, substitutions),
            ExprKind::GenericMember { object, args, .. } => {
                self.rewrite_expr(object, substitutions);
                for arg in args {
                    self.rewrite_type(arg, substitutions);
                }
            }
            ExprKind::StructInit { name, args, fields } => {
                for arg in args.iter_mut() {
                    self.rewrite_type(arg, substitutions);
                }
                if self.struct_templates.contains_key(name) && !args.is_empty() {
                    self.apply_struct_field_contexts(name, args, fields, substitutions);
                }
                for field in fields.iter_mut() {
                    self.rewrite_expr(&mut field.value, substitutions);
                }
                if self.struct_templates.contains_key(name) {
                    if args.is_empty() {
                        match self.infer_struct_type_arguments(name, fields) {
                            Ok(mut inferred_args) => {
                                for inferred_arg in &mut inferred_args {
                                    self.rewrite_type(inferred_arg, substitutions);
                                }
                                *args = inferred_args;
                            }
                            Err(message) => {
                                self.diagnostics.push(Diagnostic::error(expr.span, message));
                            }
                        }
                    }
                    if !args.is_empty() {
                        self.specialize_struct(name, args, expr.span);
                        *name = specialized_name(name, args);
                        args.clear();
                    }
                } else if self.concrete_structs.contains(name) && !args.is_empty() {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("struct `{name}` does not accept type arguments"),
                    ));
                }
            }
            ExprKind::Range { start, end, .. } => {
                let endpoint = TypeRef {
                    name: "i32".to_string(),
                    args: Vec::new(),
                    bindings: Vec::new(),
                    function: None,
                    span: expr.span,
                };
                self.apply_expr_context(start, &endpoint);
                self.apply_expr_context(end, &endpoint);
                self.rewrite_expr(start, substitutions);
                self.rewrite_expr(end, substitutions);
            }
            ExprKind::Binary { left, right, .. } => {
                self.rewrite_expr(left, substitutions);
                self.rewrite_expr(right, substitutions);
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.rewrite_expr(operand, substitutions);
            }
            ExprKind::Cast { value, type_ref } => {
                self.rewrite_type(type_ref, substitutions);
                self.rewrite_expr(value, substitutions);
            }
            ExprKind::Match { value, branches } => {
                self.rewrite_expr(value, substitutions);
                let mut value_type = self.infer_expr_type(value);
                if let Some(type_ref) = &mut value_type
                    && self.enum_templates.contains_key(&type_ref.name)
                    && !type_ref.args.is_empty()
                {
                    let name = type_ref.name.clone();
                    self.specialize_enum(&name, &type_ref.args, type_ref.span);
                    type_ref.name = specialized_name(&name, &type_ref.args);
                    type_ref.args.clear();
                }
                if let Some(type_ref) = &mut value_type
                    && self.struct_templates.contains_key(&type_ref.name)
                    && !type_ref.args.is_empty()
                {
                    let name = type_ref.name.clone();
                    self.specialize_struct(&name, &type_ref.args, type_ref.span);
                    type_ref.name = specialized_name(&name, &type_ref.args);
                    type_ref.args.clear();
                }
                for branch in branches {
                    let mut scope = HashMap::new();
                    if let Some(value_type) = &value_type {
                        self.rewrite_match_pattern(&mut branch.pattern, value_type, &mut scope);
                    }
                    self.scopes.push(scope);
                    if let Some(guard) = &mut branch.guard {
                        self.rewrite_expr(guard, substitutions);
                    }
                    match &mut branch.body {
                        MatchBranchBody::Expr(expr) => self.rewrite_expr(expr, substitutions),
                        MatchBranchBody::Block(block) => self.rewrite_block(block, substitutions),
                    }
                    self.scopes.pop();
                }
            }
            ExprKind::Lambda(function) => self.rewrite_function(function, substitutions),
            ExprKind::Identifier(_)
            | ExprKind::GenericType { .. }
            | ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Bool(_)
            | ExprKind::Missing => {}
        }
    }

    fn rewrite_match_pattern(
        &mut self,
        pattern: &mut Pattern,
        value_type: &TypeRef,
        scope: &mut HashMap<String, TypeRef>,
    ) {
        match pattern {
            Pattern::Or { alternatives, .. } => {
                for alternative in alternatives {
                    self.rewrite_match_pattern(alternative, value_type, scope);
                }
            }
            Pattern::Variant {
                enum_name,
                variant,
                payload,
                ..
            } => {
                if let Some((generic_name, _)) = self.specializations.get(&value_type.name)
                    && enum_name == generic_name
                    && self.enum_templates.contains_key(generic_name)
                {
                    *enum_name = value_type.name.clone();
                }
                let Some(Some(mut payload_type)) = self.enum_variant_payload(enum_name, variant)
                else {
                    return;
                };
                self.specialize_match_payload_type(&mut payload_type);
                if let Some(payload) = payload {
                    self.rewrite_match_pattern(payload, &payload_type, scope);
                }
            }
            Pattern::Struct { name, fields, .. } => {
                let substitutions = if let Some((generic_name, args)) =
                    self.specializations.get(&value_type.name).cloned()
                    && name == &generic_name
                    && self.struct_templates.contains_key(&generic_name)
                {
                    *name = value_type.name.clone();
                    self.struct_templates[&generic_name]
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>()
                } else {
                    HashMap::new()
                };

                for field in fields {
                    let Some(mut field_type) = self.match_struct_field_type(name, &field.name)
                    else {
                        continue;
                    };
                    if !substitutions.is_empty() {
                        field_type = substitute_type(&field_type, &substitutions);
                    }
                    self.rewrite_type(&mut field_type, &HashMap::new());
                    self.rewrite_match_pattern(&mut field.pattern, &field_type, scope);
                }
            }
            Pattern::Binding { name, .. } if name != "_" => {
                scope.insert(name.clone(), value_type.clone());
            }
            Pattern::Binding { .. }
            | Pattern::String { .. }
            | Pattern::Bool { .. }
            | Pattern::Number { .. }
            | Pattern::Range { .. }
            | Pattern::Wildcard { .. } => {}
        }
    }

    fn specialize_match_payload_type(&mut self, type_ref: &mut TypeRef) {
        if self.enum_templates.contains_key(&type_ref.name) && !type_ref.args.is_empty() {
            let name = type_ref.name.clone();
            self.specialize_enum(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        }
        if self.struct_templates.contains_key(&type_ref.name) && !type_ref.args.is_empty() {
            let name = type_ref.name.clone();
            self.specialize_struct(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        }
    }

    fn match_struct_field_type(&self, struct_name: &str, field_name: &str) -> Option<TypeRef> {
        let definition = self
            .concrete_struct_defs
            .get(struct_name)
            .or_else(|| self.struct_templates.get(struct_name))
            .or_else(|| {
                self.specializations
                    .get(struct_name)
                    .and_then(|(generic_name, _)| self.struct_templates.get(generic_name))
            })?;
        definition.members.iter().find_map(|member| {
            let StructMember::Field(field) = member else {
                return None;
            };
            (field.name == field_name).then(|| field.type_ref.clone())
        })
    }

}
