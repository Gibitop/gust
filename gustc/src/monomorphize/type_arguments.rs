impl Monomorphizer {
    fn apply_expr_context(&mut self, expr: &mut Expr, expected: &TypeRef) {
        self.expected_expr_types.insert(expr.span, expected.clone());
        let Some((generic_name, concrete_args)) = self.specializations.get(&expected.name) else {
            return;
        };
        match &mut expr.kind {
            ExprKind::StructInit { name, args, .. } if args.is_empty() && name == generic_name => {
                *args = concrete_args.clone();
            }
            ExprKind::Member { object, .. } => {
                if let ExprKind::Identifier(name) = &object.kind
                    && name == generic_name
                {
                    object.kind = ExprKind::GenericType {
                        name: name.clone(),
                        args: concrete_args.clone(),
                    };
                }
            }
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Member { object, .. } = &mut callee.kind
                    && let ExprKind::Identifier(name) = &object.kind
                    && name == generic_name
                {
                    object.kind = ExprKind::GenericType {
                        name: name.clone(),
                        args: concrete_args.clone(),
                    };
                }
            }
            _ => {}
        }
    }

    fn apply_struct_field_contexts(
        &mut self,
        name: &str,
        args: &[TypeRef],
        fields: &mut [crate::ast::StructInitField],
        substitutions: &HashMap<String, TypeRef>,
    ) {
        let template = self.struct_templates[name].clone();
        let field_substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for field in fields {
            let Some(mut expected) = template.members.iter().find_map(|member| {
                let StructMember::Field(expected) = member else {
                    return None;
                };
                (expected.name == field.name)
                    .then(|| substitute_type(&expected.type_ref, &field_substitutions))
            }) else {
                continue;
            };
            self.rewrite_type(&mut expected, substitutions);
            self.apply_expr_context(&mut field.value, &expected);
        }
    }

    fn infer_struct_type_arguments(
        &self,
        name: &str,
        fields: &[crate::ast::StructInitField],
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.struct_templates[name];
        let mut constraints = Vec::new();
        for field in fields {
            let Some(expected) = template.members.iter().find_map(|member| {
                let StructMember::Field(expected) = member else {
                    return None;
                };
                (expected.name == field.name).then_some(&expected.type_ref)
            }) else {
                continue;
            };
            if let Some(actual) = self.infer_expr_type(&field.value) {
                constraints.push((expected.clone(), actual));
            }
        }
        self.solve_type_arguments(name, &template.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic struct `{name}`: {reason}; write `{name}<Type> {{ ... }}` or add a concrete type annotation"
                )
            })
    }

    fn infer_static_type_arguments(
        &self,
        type_name: &str,
        method_name: &str,
        args: &[Expr],
    ) -> Result<Vec<TypeRef>, String> {
        let (type_params, members, kind) =
            if let Some(template) = self.struct_templates.get(type_name) {
                (&template.type_params, &template.members, "struct")
            } else if let Some(template) = self.enum_templates.get(type_name) {
                (&template.type_params, &template.members, "enum")
            } else {
                return Err(format!("unknown generic type `{type_name}`"));
            };
        let Some(function) = find_method_member(members, method_name, true) else {
            return Err(format!(
                "unknown static function `{method_name}` for generic {kind} `{type_name}`"
            ));
        };
        let constraints = function
            .params
            .iter()
            .filter_map(|param| param.type_ref.as_ref())
            .zip(args)
            .filter_map(|(expected, arg)| {
                self.infer_expr_type(arg)
                    .map(|actual| (expected.clone(), actual))
            })
            .collect();
        self.solve_type_arguments(type_name, type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic static call `{type_name}.{method_name}`: {reason}; write `{type_name}<Type>.{method_name}(...)` or add a concrete expected type"
                )
            })
    }

    fn infer_enum_type_arguments(
        &self,
        type_name: &str,
        variant_name: &str,
        args: &[Expr],
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.enum_templates[type_name];
        let Some(variant) = template
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
        else {
            return Err(format!("unknown variant `{type_name}.{variant_name}`"));
        };
        let expected_count = usize::from(variant.payload.is_some());
        if args.len() != expected_count {
            return Err(format!(
                "enum variant `{type_name}.{variant_name}` expects {expected_count} arguments, got {}",
                args.len()
            ));
        }
        let constraints = variant
            .payload
            .iter()
            .zip(args)
            .filter_map(|(expected, arg)| {
                self.infer_expr_type(arg)
                    .map(|actual| (expected.clone(), actual))
            })
            .collect();
        self.solve_type_arguments(type_name, &template.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic enum `{type_name}`: {reason}; write `{type_name}<Type>.{variant_name}(...)` or add a concrete expected type"
                )
            })
    }

    fn has_real_or_extension_method(
        &self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
    ) -> bool {
        if self
            .method_template(receiver, method_name, static_)
            .is_some()
        {
            return true;
        }

        let receiver = self.expanded_type(receiver);
        self.extensions.iter().any(|extension| {
            if extension.static_ != static_
                || extension.function.name.as_deref() != Some(method_name)
            {
                return false;
            }
            let receiver_type_params = self.extension_receiver_type_params(extension);
            self.solve_type_arguments(
                "extension",
                &receiver_type_params,
                vec![(extension.type_ref.clone(), receiver.clone())],
            )
            .is_ok()
        })
    }

    fn resolve_generic_trait_method(
        &self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        args: &[Expr],
        expected_return: Option<&TypeRef>,
    ) -> Result<Option<GenericTraitMethodResolution>, String> {
        let mut candidates = Vec::new();
        let (requested_trait, source_method_name) = requested_trait_method(method_name);

        for impl_ in &self.impl_declarations {
            let Some(trait_) = self.trait_declarations.get(&impl_.trait_ref.name) else {
                continue;
            };
            if requested_trait.is_some_and(|requested| {
                !trait_name_matches_request(&trait_.name, requested)
            }) {
                continue;
            }
            if trait_.type_params.is_empty() && trait_.associated_types.is_empty() {
                continue;
            }
            let Some(method) = trait_
                .methods
                .iter()
                .find(|method| method.name == source_method_name && method.static_ == static_)
            else {
                continue;
            };
            let Some(method_return_type) = &method.return_type else {
                continue;
            };
            let method_params = method
                .params
                .iter()
                .filter(|param| static_ || param.name != "self")
                .collect::<Vec<_>>();
            if method_params.len() != args.len() {
                continue;
            }

            let mut trait_substitutions = trait_
                .type_params
                .iter()
                .cloned()
                .zip(impl_.trait_ref.args.iter().cloned())
                .collect::<HashMap<_, _>>();
            trait_substitutions.insert("Self".to_string(), impl_.type_ref.clone());
            trait_substitutions.extend(
                impl_
                    .associated_types
                    .iter()
                    .filter(|associated_type| associated_type.type_params.is_empty())
                    .map(|associated_type| {
                        (
                            format!("Self.{}", associated_type.name),
                            associated_type.type_ref.clone(),
                        )
                    }),
            );

            let mut constraints = vec![(impl_.type_ref.clone(), receiver.clone())];
            for (param, arg) in method_params.iter().zip(args) {
                let Some(expected) = &param.type_ref else {
                    continue;
                };
                if matches!(arg.kind, ExprKind::Number(_)) {
                    continue;
                }
                let Some(actual) = self.infer_expr_type(arg) else {
                    continue;
                };
                constraints.push((substitute_type(expected, &trait_substitutions), actual));
            }
            if let (Some(return_type), Some(expected_return)) =
                (&method.return_type, expected_return)
            {
                constraints.push((
                    substitute_type(return_type, &trait_substitutions),
                    expected_return.clone(),
                ));
            }

            let Ok(type_args) =
                self.solve_type_arguments("trait method", &impl_.type_params, constraints.clone())
            else {
                continue;
            };
            let impl_substitutions = impl_
                .type_params
                .iter()
                .cloned()
                .zip(type_args.iter().cloned())
                .collect::<HashMap<_, _>>();
            if !constraints.iter().all(|(pattern, actual)| {
                let pattern = self.expanded_type(&substitute_type(pattern, &impl_substitutions));
                let actual = self.expanded_type(actual);
                type_name(&pattern) == type_name(&actual)
            }) {
                continue;
            }

            candidates.push(GenericTraitMethodResolution {
                trait_name: trait_.name.clone(),
                trait_args: impl_
                    .trait_ref
                    .args
                    .iter()
                    .map(|arg| substitute_type(arg, &impl_substitutions))
                    .collect(),
                associated_type_bindings: impl_
                    .associated_types
                    .iter()
                    .filter(|associated_type| associated_type.type_params.is_empty())
                    .map(|associated_type| crate::ast::AssociatedTypeBinding {
                        name: associated_type.name.clone(),
                        type_ref: substitute_type(
                            &associated_type.type_ref,
                            &impl_substitutions,
                        ),
                        span: associated_type.span,
                    })
                    .collect(),
                params: method_params
                    .iter()
                    .filter_map(|param| param.type_ref.as_ref())
                    .map(|param| {
                        substitute_type(
                            &substitute_type(param, &trait_substitutions),
                            &impl_substitutions,
                        )
                    })
                    .collect(),
                return_type: substitute_type(
                    &substitute_type(method_return_type, &trait_substitutions),
                    &impl_substitutions,
                ),
                impl_type_params: impl_.type_params.clone(),
                impl_type_param_bounds: impl_.type_param_bounds.clone(),
                impl_type_args: type_args,
            });
        }

        if candidates.len() > 1 {
            return Err(format!(
                "generic trait method `{source_method_name}` is ambiguous for type `{}`",
                type_name(receiver)
            ));
        }
        Ok(candidates.pop())
    }

    fn infer_function_type_arguments(
        &self,
        function_name: &str,
        args: &[Expr],
        expected_return: Option<&TypeRef>,
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.function_templates[function_name];
        let mut constraints = template
            .params
            .iter()
            .zip(args)
            .filter_map(|(param, arg)| {
                let expected = param.type_ref.as_ref()?;
                self.infer_expr_type(arg)
                    .map(|actual| (expected.clone(), actual))
            })
            .collect::<Vec<_>>();
        let return_type = template
            .return_type
            .as_ref()
            .or_else(|| self.generic_function_returns.get(function_name));
        if let (Some(return_type), Some(expected_return)) = (return_type, expected_return) {
            constraints.push((return_type.clone(), expected_return.clone()));
        }
        self.solve_type_arguments(function_name, &template.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic function `{function_name}`: {reason}; write `{function_name}<Type>(...)` or add a concrete expected type"
                )
            })
    }

    fn infer_function_value_type_arguments(
        &self,
        function_name: &str,
        expected: &TypeRef,
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.function_templates[function_name];
        let Some(expected_function) = &expected.function else {
            return Err(format!(
                "expected a function type, got `{}`",
                type_name(expected)
            ));
        };

        if template.params.len() != expected_function.params.len() {
            return Err(format!(
                "expected {} parameters, got {}",
                expected_function.params.len(),
                template.params.len()
            ));
        }

        let mut constraints = Vec::new();
        for (param, expected_param) in template.params.iter().zip(&expected_function.params) {
            if param.mutable != expected_param.mutable {
                return Err("parameter mutability does not match".to_string());
            }
            let Some(type_ref) = &param.type_ref else {
                return Err(format!(
                    "parameter `{}` does not have a type annotation",
                    param.name
                ));
            };
            constraints.push((type_ref.clone(), expected_param.type_ref.clone()));
        }

        let Some(return_type) = template
            .return_type
            .as_ref()
            .or_else(|| self.generic_function_returns.get(function_name))
        else {
            return Err("the return type could not be inferred".to_string());
        };
        constraints.push((return_type.clone(), (*expected_function.return_type).clone()));

        self.solve_type_arguments(function_name, &template.type_params, constraints)
    }

    fn specialized_function_value_type(
        &mut self,
        function_name: &str,
        type_args: &[TypeRef],
        substitutions: &HashMap<String, TypeRef>,
        span: crate::span::Span,
    ) -> Result<TypeRef, String> {
        let template = self.function_templates[function_name].clone();
        let function_substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect::<HashMap<_, _>>();
        let mut params = Vec::new();
        for param in &template.params {
            let Some(type_ref) = &param.type_ref else {
                return Err(format!(
                    "parameter `{}` does not have a type annotation",
                    param.name
                ));
            };
            let mut type_ref = substitute_type(type_ref, &function_substitutions);
            self.rewrite_type(&mut type_ref, substitutions);
            params.push(crate::ast::FunctionTypeParam {
                mutable: param.mutable,
                type_ref,
            });
        }
        let Some(return_type) = template
            .return_type
            .as_ref()
            .or_else(|| self.generic_function_returns.get(function_name))
        else {
            return Err("the return type could not be inferred".to_string());
        };
        let mut return_type = substitute_type(return_type, &function_substitutions);
        self.rewrite_type(&mut return_type, substitutions);

        Ok(TypeRef {
            name: "fn".to_string(),
            args: Vec::new(),
            bindings: Vec::new(),
            function: Some(crate::ast::FunctionTypeRef {
                params,
                return_type: Box::new(return_type),
            }),
            span,
        })
    }

    fn apply_generic_function_argument_contexts(
        &mut self,
        function_name: &str,
        type_args: &[TypeRef],
        args: &mut [Expr],
        substitutions: &HashMap<String, TypeRef>,
    ) {
        let template = self.function_templates[function_name].clone();
        let function_substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for (param, arg) in template.params.iter().zip(args) {
            let Some(type_ref) = &param.type_ref else {
                continue;
            };
            let mut expected = substitute_type(type_ref, &function_substitutions);
            self.rewrite_type(&mut expected, substitutions);
            self.apply_expr_context(arg, &expected);
        }
    }

    fn infer_method_type_arguments(
        &mut self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        args: &[Expr],
        expected_return: Option<&TypeRef>,
    ) -> Result<Vec<TypeRef>, String> {
        let (receiver, struct_substitutions, function) = self
            .method_template(receiver, method_name, static_)
            .ok_or_else(|| format!("unknown generic method `{method_name}`"))?;
        let mut constraints = function
            .params
            .iter()
            .filter(|param| !(param.name == "self" && param.type_ref.is_none()))
            .zip(args)
            .filter_map(|(param, arg)| {
                let expected = param.type_ref.as_ref()?;
                self.infer_expr_type(arg)
                    .map(|actual| (substitute_type(expected, &struct_substitutions), actual))
            })
            .collect::<Vec<_>>();
        let return_type = self.method_return_type(&receiver, &function, static_);
        if let (Some(return_type), Some(expected_return)) = (return_type, expected_return) {
            constraints.push((return_type, expected_return.clone()));
        }
        self.solve_type_arguments(method_name, &function.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic method `{}.{method_name}`: {reason}; write `.{method_name}<Type>(...)` or add a concrete expected type",
                    receiver.name
                )
            })
    }

    fn apply_generic_method_argument_contexts(
        &mut self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        type_args: &[TypeRef],
        args: &mut [Expr],
        substitutions: &HashMap<String, TypeRef>,
    ) {
        let Some((_, mut method_substitutions, function)) =
            self.method_template(receiver, method_name, static_)
        else {
            return;
        };
        method_substitutions.extend(
            function
                .type_params
                .iter()
                .cloned()
                .zip(type_args.iter().cloned()),
        );
        for (param, arg) in function
            .params
            .iter()
            .filter(|param| !(param.name == "self" && param.type_ref.is_none()))
            .zip(args)
        {
            let Some(type_ref) = &param.type_ref else {
                continue;
            };
            let mut expected = substitute_type(type_ref, &method_substitutions);
            self.rewrite_type(&mut expected, substitutions);
            self.apply_expr_context(arg, &expected);
        }
    }

    fn resolve_extension(
        &mut self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        explicit_args: Option<&[TypeRef]>,
        args: &[Expr],
        expected_return: Option<&TypeRef>,
    ) -> Result<Option<ExtensionResolution>, String> {
        let receiver = self.expanded_type(receiver);
        let mut candidates = Vec::new();

        for (template_index, extension) in self.extensions.clone().iter().enumerate() {
            if extension.static_ != static_
                || extension.function.name.as_deref() != Some(method_name)
                || (!self.is_generic_extension_template(extension)
                    && template_index < self.trait_default_extension_start)
            {
                continue;
            }

            let receiver_type_params = self.extension_receiver_type_params(extension);
            let Ok(receiver_type_args) = self.solve_type_arguments(
                "extension",
                &receiver_type_params,
                vec![(extension.type_ref.clone(), receiver.clone())],
            ) else {
                continue;
            };
            let mut substitutions = receiver_type_params
                .iter()
                .cloned()
                .zip(receiver_type_args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let function_params = extension
                .function
                .params
                .iter()
                .filter(|param| static_ || param.name != "self")
                .filter_map(|param| param.type_ref.as_ref())
                .map(|type_ref| substitute_type(type_ref, &substitutions))
                .collect::<Vec<_>>();

            if function_params.len() != args.len() {
                continue;
            }

            let function_type_args = if let Some(explicit_args) = explicit_args {
                if explicit_args.len() != extension.function.type_params.len() {
                    return Err(format!(
                        "generic extension method `{}.{method_name}` expects {} type arguments, got {}",
                        type_name(&receiver),
                        extension.function.type_params.len(),
                        explicit_args.len()
                    ));
                }
                explicit_args.to_vec()
            } else if extension.function.type_params.is_empty() {
                Vec::new()
            } else {
                let mut constraints = function_params
                    .iter()
                    .zip(args)
                    .filter_map(|(expected, arg)| {
                        self.infer_extension_argument_type(expected, arg)
                            .map(|actual| (expected.clone(), actual))
                    })
                    .collect::<Vec<_>>();
                if let (Some(return_type), Some(expected_return)) =
                    (&extension.function.return_type, expected_return)
                {
                    constraints.push((substitute_type(return_type, &substitutions), expected_return.clone()));
                }
                let mut type_args = self.solve_type_arguments(
                    method_name,
                    &extension.function.type_params,
                    constraints,
                )
                .map_err(|reason| {
                    format!(
                        "cannot infer type arguments for generic extension method `{}.{method_name}`: {reason}; write `.{method_name}<Type>(...)` or add a concrete expected type",
                        type_name(&receiver)
                    )
                })?;
                for type_arg in &mut type_args {
                    self.rewrite_type(type_arg, &HashMap::new());
                }
                type_args
            };

            substitutions.extend(
                extension
                    .function
                    .type_params
                    .iter()
                    .cloned()
                    .zip(function_type_args.iter().cloned()),
            );

            let params = function_params
                .iter()
                .map(|param| substitute_type(param, &substitutions))
                .collect::<Vec<_>>();
            let return_type = extension
                .function
                .return_type
                .as_ref()
                .map(|return_type| substitute_type(return_type, &substitutions));
            let mut concrete_receiver = receiver.clone();
            self.rewrite_type(&mut concrete_receiver, &HashMap::new());

            candidates.push(ExtensionResolution {
                template_index,
                receiver: concrete_receiver,
                receiver_type_params,
                receiver_type_args,
                function_type_args,
                params,
                return_type,
            });
        }

        candidates.sort_by_key(|candidate| candidate.receiver_type_params.len());
        if candidates.len() > 1 {
            let first_len = candidates[0].receiver_type_params.len();
            if candidates
                .get(1)
                .is_some_and(|candidate| candidate.receiver_type_params.len() == first_len)
            {
                return Err(format!(
                    "extension method `{method_name}` is ambiguous for type `{}`",
                    type_name(&receiver)
                ));
            }
        }

        Ok(candidates.into_iter().next())
    }

    fn infer_extension_argument_type(
        &mut self,
        expected: &TypeRef,
        arg: &Expr,
    ) -> Option<TypeRef> {
        let ExprKind::Lambda(function) = &arg.kind else {
            return self.infer_expr_type(arg);
        };
        let expected_function = expected.function.as_ref()?;
        if function.params.len() != expected_function.params.len() {
            return None;
        }

        let scope = function
            .params
            .iter()
            .zip(&expected_function.params)
            .map(|(param, expected_param)| (param.name.clone(), expected_param.type_ref.clone()))
            .collect();
        self.scopes.push(scope);
        let return_type = function.return_type.clone().or_else(|| match &function.body {
            FunctionBody::Expr(expr) => self.infer_expr_type(expr),
            FunctionBody::Block(_) => None,
        });
        self.scopes.pop();

        Some(TypeRef {
            name: "fn".to_string(),
            args: Vec::new(),
            bindings: Vec::new(),
            function: Some(crate::ast::FunctionTypeRef {
                params: function
                    .params
                    .iter()
                    .zip(&expected_function.params)
                    .map(|(param, expected_param)| crate::ast::FunctionTypeParam {
                        mutable: param.mutable,
                        type_ref: expected_param.type_ref.clone(),
                    })
                    .collect(),
                return_type: Box::new(return_type?),
            }),
            span: arg.span,
        })
    }

    fn apply_extension_argument_contexts(
        &mut self,
        params: &[TypeRef],
        args: &mut [Expr],
        substitutions: &HashMap<String, TypeRef>,
    ) {
        for (param, arg) in params.iter().zip(args) {
            let mut expected = param.clone();
            self.rewrite_type(&mut expected, substitutions);
            self.apply_expr_context(arg, &expected);
        }
    }

    fn validate_method_type_arguments(
        &mut self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        args: &[TypeRef],
        span: crate::span::Span,
    ) -> bool {
        let Some((receiver, _, function)) = self.method_template(receiver, method_name, static_)
        else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("unknown generic method `{method_name}`"),
            ));
            return false;
        };
        let expected = function.type_params.len();
        if args.len() == expected {
            return true;
        }
        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "generic method `{}.{method_name}` expects {expected} type arguments, got {}",
                receiver.name,
                args.len()
            ),
        ));
        false
    }

    fn method_template(
        &self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
    ) -> Option<(TypeRef, HashMap<String, TypeRef>, FunctionDecl)> {
        let receiver = self.expanded_type(receiver);
        if let Some((generic_name, args)) = self.specializations.get(&receiver.name) {
            let (type_params, members) =
                if let Some(template) = self.struct_templates.get(generic_name) {
                    (&template.type_params, &template.members)
                } else if let Some(template) = self.enum_templates.get(generic_name) {
                    (&template.type_params, &template.members)
                } else {
                    return None;
                };
            let substitutions = type_params
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let mut function = find_method_member(members, method_name, static_)?;
            if function.return_type.is_none()
                && let Some(return_type) = self.generic_method_returns.get(&(
                    generic_name.clone(),
                    method_name.to_string(),
                    static_,
                ))
            {
                function.return_type = Some(return_type.clone());
            }
            return Some((receiver, substitutions, function));
        }

        if !receiver.args.is_empty()
            && (self.struct_templates.contains_key(&receiver.name)
                || self.enum_templates.contains_key(&receiver.name))
        {
            let (type_params, members) =
                if let Some(template) = self.struct_templates.get(&receiver.name) {
                    (&template.type_params, &template.members)
                } else {
                    let template = self.enum_templates.get(&receiver.name)?;
                    (&template.type_params, &template.members)
                };
            let substitutions = type_params
                .iter()
                .cloned()
                .zip(receiver.args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let mut function = find_method_member(members, method_name, static_)?;
            if function.return_type.is_none()
                && let Some(return_type) = self.generic_method_returns.get(&(
                    receiver.name.clone(),
                    method_name.to_string(),
                    static_,
                ))
            {
                function.return_type = Some(return_type.clone());
            }
            return Some((
                TypeRef {
                    name: specialized_name(&receiver.name, &receiver.args),
                    args: Vec::new(),
                    bindings: Vec::new(),
                    function: None,
                    span: receiver.span,
                },
                substitutions,
                function,
            ));
        }

        let members = if let Some(template) = self.concrete_struct_defs.get(&receiver.name) {
            &template.members
        } else if let Some(template) = self.concrete_enums.get(&receiver.name) {
            &template.members
        } else {
            return None;
        };
        let mut function = find_method_member(members, method_name, static_)?;
        if function.return_type.is_none()
            && let Some(return_type) = self.generic_method_returns.get(&(
                receiver.name.clone(),
                method_name.to_string(),
                static_,
            ))
        {
            function.return_type = Some(return_type.clone());
        }
        Some((receiver, HashMap::new(), function))
    }

    fn method_return_type(
        &mut self,
        receiver: &TypeRef,
        function: &FunctionDecl,
        static_: bool,
    ) -> Option<TypeRef> {
        function
            .return_type
            .clone()
            .or_else(|| self.infer_rewritten_function_return(function, &receiver.name, !static_))
    }

    fn validate_function_type_arguments(
        &mut self,
        function_name: &str,
        args: &[TypeRef],
        span: crate::span::Span,
    ) -> bool {
        let expected = self.function_templates[function_name].type_params.len();
        if args.len() == expected {
            return true;
        }
        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "generic function `{function_name}` expects {expected} type arguments, got {}",
                args.len()
            ),
        ));
        false
    }

    fn solve_type_arguments(
        &self,
        _type_name: &str,
        params: &[String],
        constraints: Vec<(TypeRef, TypeRef)>,
    ) -> Result<Vec<TypeRef>, String> {
        let param_names = params.iter().cloned().collect::<HashSet<_>>();
        let mut inferred = HashMap::new();
        for (expected, actual) in constraints {
            self.unify_type(&expected, &actual, &param_names, &mut inferred)?;
        }
        let missing = params
            .iter()
            .filter(|param| !inferred.contains_key(*param))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "no concrete type was found for {}",
                missing
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(params
            .iter()
            .map(|param| inferred[param].clone())
            .collect::<Vec<_>>())
    }

    fn unify_type(
        &self,
        expected: &TypeRef,
        actual: &TypeRef,
        params: &HashSet<String>,
        inferred: &mut HashMap<String, TypeRef>,
    ) -> Result<(), String> {
        let actual = self.expanded_type(actual);
        if expected.args.is_empty() && params.contains(&expected.name) {
            if let Some(previous) = inferred.get(&expected.name)
                && type_name(&self.expanded_type(previous)) != type_name(&actual)
            {
                return Err(format!(
                    "conflicting types `{}` and `{}` were inferred for `{}`",
                    type_name(&self.expanded_type(previous)),
                    type_name(&actual),
                    expected.name
                ));
            }
            inferred.insert(expected.name.clone(), actual);
            return Ok(());
        }

        if let Some(expected_function) = &expected.function {
            let Some(actual_function) = &actual.function else {
                return Ok(());
            };
            if expected_function.params.len() != actual_function.params.len() {
                return Ok(());
            }
            for (expected_param, actual_param) in expected_function
                .params
                .iter()
                .zip(&actual_function.params)
            {
                if expected_param.mutable != actual_param.mutable {
                    return Ok(());
                }
                self.unify_type(
                    &expected_param.type_ref,
                    &actual_param.type_ref,
                    params,
                    inferred,
                )?;
            }
            return self.unify_type(
                &expected_function.return_type,
                &actual_function.return_type,
                params,
                inferred,
            );
        }

        let expected = self.expanded_trait_type(expected);
        let actual = self.expanded_trait_type(&actual);
        if expected.name != actual.name || expected.args.len() != actual.args.len() {
            return Ok(());
        }
        for (expected, actual) in expected.args.iter().zip(&actual.args) {
            self.unify_type(expected, actual, params, inferred)?;
        }
        for expected_binding in &expected.bindings {
            let Some(actual_binding) = actual
                .bindings
                .iter()
                .find(|binding| binding.name == expected_binding.name)
            else {
                return Ok(());
            };
            self.unify_type(
                &expected_binding.type_ref,
                &actual_binding.type_ref,
                params,
                inferred,
            )?;
        }
        Ok(())
    }

    fn expanded_trait_type(&self, type_ref: &TypeRef) -> TypeRef {
        let type_ref = self.expanded_type(type_ref);
        if type_ref.args.is_empty()
            && type_ref.bindings.is_empty()
            && let Some((name, args, bindings)) = self.trait_specializations.get(&type_ref.name)
        {
            return TypeRef {
                name: name.clone(),
                args: args.clone(),
                bindings: bindings.clone(),
                function: None,
                span: type_ref.span,
            };
        }
        type_ref
    }

}
