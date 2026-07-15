impl Monomorphizer {
    fn specialize_method(
        &mut self,
        receiver: &str,
        method_name: &str,
        static_: bool,
        args: &[TypeRef],
    ) {
        if args.iter().any(|arg| !self.type_ref_is_fully_known(arg)) {
            return;
        }
        let specialized_method = specialized_name(method_name, args);
        let key = (receiver.to_string(), specialized_method.clone(), static_);
        let receiver_type = TypeRef {
            name: receiver.to_string(),
            args: Vec::new(),
            bindings: Vec::new(),
            function: None,
            span: args
                .first()
                .map_or_else(|| crate::span::Span::new(0, 0), |arg| arg.span),
        };
        if let Some((_, _, function)) = self.method_template(&receiver_type, method_name, static_) {
            self.record_type_param_bound_checks(
                format!("generic method `{receiver}.{method_name}`"),
                &function.type_params,
                &function.type_param_bounds,
                args,
                receiver_type.span,
            );
        }
        if !self.member_returns.contains_key(&key)
            && let Some((_, mut substitutions, function)) =
                self.method_template(&receiver_type, method_name, static_)
        {
            substitutions.extend(
                function
                    .type_params
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned()),
            );
            if let Some(return_type) = self.method_return_type(&receiver_type, &function, static_) {
                self.member_returns
                    .insert(key, substitute_type(&return_type, &substitutions));
            }
        }
        self.pending.push_back(PendingSpecialization::Method {
            receiver: receiver.to_string(),
            name: method_name.to_string(),
            static_,
            args: args.to_vec(),
        });
    }

    fn emit_method_specialization(
        &mut self,
        items: &mut [Item],
        receiver: &str,
        method_name: &str,
        static_: bool,
        args: &[TypeRef],
    ) {
        let receiver_type = TypeRef {
            name: receiver.to_string(),
            args: Vec::new(),
            bindings: Vec::new(),
            function: None,
            span: args
                .first()
                .map_or_else(|| crate::span::Span::new(0, 0), |arg| arg.span),
        };
        let Some((_, mut substitutions, mut function)) =
            self.method_template(&receiver_type, method_name, static_)
        else {
            return;
        };
        substitutions.extend(
            function
                .type_params
                .iter()
                .cloned()
                .zip(args.iter().cloned()),
        );
        function.name = Some(specialized_name(method_name, args));
        function.type_params.clear();
        function.type_param_bounds.clear();
        self.self_types.push(receiver_type.clone());
        self.rewrite_function(&mut function, &substitutions);
        self.self_types.pop();
        if function.return_type.is_none()
            && let Some(return_type) =
                self.infer_rewritten_function_return(&function, receiver, !static_)
        {
            function.return_type = Some(return_type);
        }
        if let Some(return_type) = &function.return_type {
            self.member_returns.insert(
                (
                    receiver.to_string(),
                    function.name.clone().unwrap_or_default(),
                    static_,
                ),
                return_type.clone(),
            );
        }
        let Some(item) = items.iter_mut().find(|item| {
            matches!(item, Item::Struct(struct_) if struct_.name == receiver)
                || matches!(item, Item::Enum(enum_) if enum_.name == receiver)
        }) else {
            return;
        };
        let members = match item {
            Item::Struct(struct_) => &mut struct_.members,
            Item::Enum(enum_) => &mut enum_.members,
            _ => unreachable!("method receiver must be a struct or enum"),
        };
        if static_ {
            members.push(StructMember::StaticMethod(function));
        } else {
            members.push(StructMember::Method(function));
        }
    }

    fn specialize_struct(&mut self, name: &str, args: &[TypeRef], span: crate::span::Span) {
        let expected = self.struct_templates[name].type_params.len();
        if args.len() != expected {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "generic struct `{name}` expects {expected} type arguments, got {}",
                    args.len()
                ),
            ));
            return;
        }
        if args.iter().any(|arg| !self.type_ref_is_fully_known(arg)) {
            return;
        }

        let template = self.struct_templates[name].clone();
        self.record_type_param_bound_checks(
            format!("generic struct `{name}`"),
            &template.type_params,
            &template.type_param_bounds,
            args,
            span,
        );

        self.pending.push_back(PendingSpecialization::Struct(
            name.to_string(),
            args.to_vec(),
        ));
        self.specializations.insert(
            specialized_name(name, args),
            (name.to_string(), args.to_vec()),
        );
    }

    fn specialize_enum(&mut self, name: &str, args: &[TypeRef], span: crate::span::Span) {
        let expected = self.enum_templates[name].type_params.len();
        if args.len() != expected {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "generic enum `{name}` expects {expected} type arguments, got {}",
                    args.len()
                ),
            ));
            return;
        }
        if args.iter().any(|arg| !self.type_ref_is_fully_known(arg)) {
            return;
        }

        let template = self.enum_templates[name].clone();
        self.record_type_param_bound_checks(
            format!("generic enum `{name}`"),
            &template.type_params,
            &template.type_param_bounds,
            args,
            span,
        );

        self.pending
            .push_back(PendingSpecialization::Enum(name.to_string(), args.to_vec()));
        self.specializations.insert(
            specialized_name(name, args),
            (name.to_string(), args.to_vec()),
        );
    }

    fn specialize_trait(
        &mut self,
        name: &str,
        args: &[TypeRef],
        bindings: &[crate::ast::AssociatedTypeBinding],
        span: crate::span::Span,
    ) {
        let Some(template) = self.trait_declarations.get(name).cloned() else {
            return;
        };
        if args.is_empty() && bindings.is_empty() {
            return;
        }
        let expected = template.type_params.len();
        if args.len() != expected {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "generic trait `{name}` expects {expected} type arguments, got {}",
                    args.len()
                ),
            ));
            return;
        }
        if args.iter().any(|arg| !self.type_ref_is_fully_known(arg))
            || bindings
                .iter()
                .any(|binding| !self.type_ref_is_fully_known(&binding.type_ref))
        {
            return;
        }

        self.record_type_param_bound_checks(
            format!("generic trait `{name}`"),
            &template.type_params,
            &template.type_param_bounds,
            args,
            span,
        );

        if bindings.is_empty() && trait_requires_associated_bindings(&template) {
            return;
        }

        let mut substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();
        substitutions.extend(bindings.iter().map(|binding| {
            (
                format!("Self.{}", binding.name),
                binding.type_ref.clone(),
            )
        }));
        let specialized_trait = specialized_trait_name(name, args, bindings);
        self.trait_specializations.insert(
            specialized_trait.clone(),
            (name.to_string(), args.to_vec(), bindings.to_vec()),
        );
        for method in &template.methods {
            if let Some(return_type) = &method.return_type {
                self.trait_method_returns.insert(
                    (specialized_trait.clone(), method.name.clone()),
                    substitute_type(return_type, &substitutions),
                );
            }
        }

        self.pending.push_back(PendingSpecialization::Trait(
            name.to_string(),
            args.to_vec(),
            bindings.to_vec(),
        ));
        if bindings.is_empty() {
            self.specializations.insert(
                specialized_name(name, args),
                (name.to_string(), args.to_vec()),
            );
        }
    }

    fn specialize_function(&mut self, name: &str, args: &[TypeRef]) {
        if args.iter().any(|arg| !self.type_ref_is_fully_known(arg)) {
            return;
        }
        let specialized_name = specialized_name(name, args);
        let template = self.function_templates[name].clone();
        self.record_type_param_bound_checks(
            format!("generic function `{name}`"),
            &template.type_params,
            &template.type_param_bounds,
            args,
            args.first()
                .map_or_else(|| crate::span::Span::new(0, 0), |arg| arg.span),
        );
        if !self.function_params.contains_key(&specialized_name) {
            let substitutions = template
                .type_params
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let mut params: Vec<Option<TypeRef>> = template
                .params
                .iter()
                .map(|param| {
                    param
                        .type_ref
                        .as_ref()
                        .map(|type_ref| substitute_type(type_ref, &substitutions))
                })
                .collect();
            for type_ref in params.iter_mut().flatten() {
                self.rewrite_type(type_ref, &substitutions);
            }
            self.function_params
                .insert(specialized_name.clone(), params);
            if let Some(return_type) = &template.return_type {
                let mut return_type = substitute_type(return_type, &substitutions);
                self.rewrite_type(&mut return_type, &substitutions);
                self.function_returns.insert(
                    specialized_name,
                    return_type,
                );
            } else if let Some(return_type) = self.generic_function_returns.get(name) {
                let mut return_type = substitute_type(return_type, &substitutions);
                self.rewrite_type(&mut return_type, &substitutions);
                self.function_returns.insert(
                    specialized_name,
                    return_type,
                );
            }
        }
        self.pending.push_back(PendingSpecialization::Function(
            name.to_string(),
            args.to_vec(),
        ));
    }

    fn specialize_extension(&mut self, resolution: &ExtensionResolution, span: crate::span::Span) {
        let extension = self.extensions[resolution.template_index].clone();
        let name = extension.function.name.as_deref().unwrap_or("<anonymous>");
        let mut type_params = resolution.receiver_type_params.clone();
        type_params.extend(extension.function.type_params.iter().cloned());
        let mut type_args = resolution.receiver_type_args.clone();
        type_args.extend(resolution.function_type_args.iter().cloned());
        self.record_type_param_bound_checks(
            format!(
                "extension method `{}.{}`",
                type_name(&resolution.receiver),
                name
            ),
            &type_params,
            &extension.type_param_bounds,
            &type_args,
            span,
        );
        self.record_type_param_bound_checks(
            format!(
                "extension method `{}.{}`",
                type_name(&resolution.receiver),
                name
            ),
            &type_params,
            &extension.function.type_param_bounds,
            &type_args,
            span,
        );
        self.pending.push_back(PendingSpecialization::Extension {
            template_index: resolution.template_index,
            receiver: resolution.receiver.clone(),
            function_args: resolution.function_type_args.clone(),
        });
    }

    fn request_impl(&mut self, trait_ref: &TypeRef, type_ref: &TypeRef) {
        let mut trait_ref = self.expanded_trait_type(trait_ref);
        for arg in &mut trait_ref.args {
            self.rewrite_type(arg, &HashMap::new());
        }
        for binding in &mut trait_ref.bindings {
            self.rewrite_type(&mut binding.type_ref, &HashMap::new());
        }
        let mut type_ref = self.expanded_type(type_ref);
        self.rewrite_type(&mut type_ref, &HashMap::new());
        if !self.type_ref_is_fully_known(&trait_ref) || !self.type_ref_is_fully_known(&type_ref) {
            return;
        }
        self.specialize_trait(
            &trait_ref.name,
            &trait_ref.args,
            &trait_ref.bindings,
            trait_ref.span,
        );
        self.pending.push_back(PendingSpecialization::Impl {
            trait_ref,
            type_ref,
        });
    }

    fn request_impl_for_expected_trait(&mut self, expected: &TypeRef, actual: &TypeRef) {
        let expected = self.expanded_trait_type(expected);
        if !self.trait_declarations.contains_key(&expected.name) {
            return;
        }
        let actual = self.expanded_trait_type(actual);
        if self.trait_declarations.contains_key(&actual.name) {
            return;
        }
        self.request_impl(&expected, &actual);
    }

    fn request_source_trait_impl(
        &mut self,
        source_trait_name: &str,
        associated_type: Option<(&str, TypeRef)>,
        type_ref: &TypeRef,
        span: crate::span::Span,
    ) {
        let Some(trait_name) = self
            .trait_declarations
            .keys()
            .find(|name| {
                *name == source_trait_name || name.rsplit("::").next() == Some(source_trait_name)
            })
            .cloned()
        else {
            return;
        };
        let bindings = associated_type
            .map(|(name, type_ref)| {
                vec![crate::ast::AssociatedTypeBinding {
                    name: name.to_string(),
                    type_ref,
                    span,
                }]
            })
            .unwrap_or_default();
        let trait_ref = TypeRef {
            name: trait_name,
            args: Vec::new(),
            bindings,
            function: None,
            span,
        };
        self.request_impl(&trait_ref, type_ref);
    }

    fn emit_extension_specialization(
        &mut self,
        items: &mut Vec<Item>,
        template_index: usize,
        receiver: &TypeRef,
        function_args: &[TypeRef],
    ) {
        let Some(template) = self.extensions.get(template_index).cloned() else {
            return;
        };
        let Some(function_name) = template.function.name.clone() else {
            return;
        };
        let emitted_name = if function_args.is_empty() {
            function_name.clone()
        } else {
            specialized_name(&function_name, function_args)
        };
        let emitted_key = format!(
            "extension {}.{}",
            type_name(receiver),
            emitted_name
        );
        if !self.emitted.insert(emitted_key) {
            return;
        }

        let receiver_type_params = self.extension_receiver_type_params(&template);
        let receiver_type_args = self
            .solve_type_arguments(
                "extension",
                &receiver_type_params,
                vec![(template.type_ref.clone(), self.expanded_trait_type(receiver))],
            )
            .unwrap_or_default();
        let mut substitutions = receiver_type_params
            .iter()
            .cloned()
            .zip(receiver_type_args)
            .collect::<HashMap<_, _>>();
        substitutions.extend(
            template
                .function
                .type_params
                .iter()
                .cloned()
                .zip(function_args.iter().cloned()),
        );
        let associated_type_substitutions = template
            .type_ref
            .bindings
            .iter()
            .map(|binding| {
                (
                    format!("Self.{}", binding.name),
                    substitute_type(&binding.type_ref, &substitutions),
                )
            })
            .collect::<Vec<_>>();
        substitutions.extend(associated_type_substitutions);

        let mut specialized = template;
        specialized.type_ref = receiver.clone();
        specialized.type_params.clear();
        specialized.type_param_bounds.clear();
        specialized.function.name = Some(emitted_name.clone());
        specialized.function.type_params.clear();
        specialized.function.type_param_bounds.clear();
        self.self_types.push(receiver.clone());
        self.rewrite_function(&mut specialized.function, &substitutions);
        self.self_types.pop();
        if specialized.function.return_type.is_none()
            && let Some(return_type) = self.infer_rewritten_function_return(
                &specialized.function,
                &receiver.name,
                !specialized.static_,
            )
        {
            specialized.function.return_type = Some(return_type);
        }
        items.push(Item::Extension(specialized));
    }
}
