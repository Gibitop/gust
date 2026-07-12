impl Monomorphizer {
    fn specialize_method(
        &mut self,
        receiver: &str,
        method_name: &str,
        static_: bool,
        args: &[TypeRef],
    ) {
        let specialized_method = specialized_name(method_name, args);
        let key = (receiver.to_string(), specialized_method.clone(), static_);
        let receiver_type = TypeRef {
            name: receiver.to_string(),
            args: Vec::new(),
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

    fn specialize_trait(&mut self, name: &str, args: &[TypeRef], span: crate::span::Span) {
        let expected = self.trait_templates[name].type_params.len();
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

        let template = self.trait_templates[name].clone();
        self.record_type_param_bound_checks(
            format!("generic trait `{name}`"),
            &template.type_params,
            &template.type_param_bounds,
            args,
            span,
        );

        self.pending.push_back(PendingSpecialization::Trait(
            name.to_string(),
            args.to_vec(),
        ));
        self.specializations.insert(
            specialized_name(name, args),
            (name.to_string(), args.to_vec()),
        );
    }

    fn specialize_function(&mut self, name: &str, args: &[TypeRef]) {
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
            let params = template
                .params
                .iter()
                .map(|param| {
                    param
                        .type_ref
                        .as_ref()
                        .map(|type_ref| substitute_type(type_ref, &substitutions))
                })
                .collect();
            self.function_params
                .insert(specialized_name.clone(), params);
            if let Some(return_type) = &template.return_type {
                self.function_returns.insert(
                    specialized_name,
                    substitute_type(return_type, &substitutions),
                );
            } else if let Some(return_type) = self.generic_function_returns.get(name) {
                self.function_returns.insert(
                    specialized_name,
                    substitute_type(return_type, &substitutions),
                );
            }
        }
        self.pending.push_back(PendingSpecialization::Function(
            name.to_string(),
            args.to_vec(),
        ));
    }
}
