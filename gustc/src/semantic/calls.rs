impl Analyzer {
    fn validate_call(&mut self, expr: &Expr, name: &str, args: &[Expr]) -> Type {
        if name == "panic" {
            if args.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("function `panic` expects 1 argument, got {}", args.len()),
                ));
                for arg in args {
                    self.validate_expr(arg);
                }
                return Type::Void;
            }

            let arg_type =
                self.validate_expr_with_context(&args[0], Some(Type::Basic(BasicType::String)));
            self.report_type_mismatch(args[0].span, Type::Basic(BasicType::String), arg_type);

            return Type::Void;
        }

        let Some(signature) = self.functions.get(name).cloned() else {
            if let Some(Binding {
                type_:
                    Type::Function {
                        params,
                        return_type,
                    },
                ..
            }) = self.lookup(name)
            {
                return self.validate_function_value_call(expr, &params, &return_type, args);
            }

            if self.values.contains(name) {
                for arg in args {
                    self.validate_expr(arg);
                }

                return Type::Unknown;
            }

            let binding = self.lookup(name);
            if binding.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown name `{name}`"),
                ));
            } else if binding.is_some_and(|binding| matches!(binding.type_, Type::Unknown)) {
                for arg in args {
                    self.validate_expr(arg);
                }

                return Type::Unknown;
            } else {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("`{name}` is not callable"),
                ));
            }

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Unknown;
        };

        if args.len() != signature.params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "function `{name}` expects {} arguments, got {}",
                    signature.params.len(),
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return signature.return_type;
        }

        for (arg, param) in args.iter().zip(signature.params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_.clone(), arg_type);

            if param.mutable
                && self.requires_mutable_capability(&param.type_)
                && !self.expr_has_mutable_capability(arg)
            {
                self.diagnostics.push(Diagnostic::error(
                    arg.span,
                    format!(
                        "function `{name}` requires a mutable argument; use `.clone()` to pass an independent mutable object"
                    ),
                ));
            }
        }

        signature.return_type
    }

    fn validate_static_call(
        &mut self,
        expr: &Expr,
        type_: Type,
        name: &str,
        args: &[Expr],
    ) -> Type {
        let source_name = source_callable_name(name);
        let requested_trait =
            requested_trait_name(name).filter(|trait_name| self.traits.contains_key(*trait_name));
        let intrinsic = if requested_trait.is_none() {
            match &type_ {
                Type::Struct(struct_name) => self
                    .structs
                    .get(struct_name)
                    .and_then(|struct_| struct_.static_methods.get(source_name))
                    .cloned(),
                Type::Enum(enum_name) => self
                    .enums
                    .get(enum_name)
                    .and_then(|enum_| enum_.static_methods.get(source_name))
                    .cloned(),
                _ => None,
            }
        } else {
            None
        };
        let signature = if let Some(trait_name) = requested_trait {
            self.qualified_static_trait_methods
                .get(&qualified_static_trait_method_name(
                    trait_name,
                    &type_.name(),
                    source_name,
                ))
                .cloned()
        } else {
            intrinsic
                .or_else(|| {
                    self.static_extensions
                        .get(&extension_name(&type_.name(), name))
                        .cloned()
                })
                .or_else(|| {
                    self.static_trait_methods
                        .get(&static_trait_method_name(&type_.name(), source_name))
                        .cloned()
                })
        };
        let Some(signature) = signature else {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "unknown static function `{source_name}` for type `{}`",
                    type_.name()
                ),
            ));
            for arg in args {
                self.validate_expr(arg);
            }
            return Type::Unknown;
        };
        let qualified_name = format!("{}.{source_name}", type_.name());

        if args.len() != signature.params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "static function `{qualified_name}` expects {} arguments, got {}",
                    signature.params.len(),
                    args.len()
                ),
            ));
            for arg in args {
                self.validate_expr(arg);
            }
            return signature.return_type;
        }

        for (arg, param) in args.iter().zip(signature.params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_, arg_type);
        }

        signature.return_type
    }

    fn validate_method_call(
        &mut self,
        expr: &Expr,
        object: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Type {
        let object_type = self.validate_expr(object);
        if matches!(object_type, Type::Unknown) {
            for arg in args {
                self.validate_expr(arg);
            }
            return Type::Unknown;
        }

        let source_name = source_callable_name(name);
        let requested_trait =
            requested_trait_name(name).filter(|trait_name| self.traits.contains_key(*trait_name));
        if matches!(&object_type, Type::Basic(type_) if type_.is_numeric())
            && source_name == "toString"
            && requested_trait.is_none()
        {
            if !args.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "method `{}.toString` expects 0 arguments, got {}",
                        object_type.name(),
                        args.len()
                    ),
                ));
                for arg in args {
                    self.validate_expr(arg);
                }
            }

            return Type::Basic(BasicType::String);
        }

        if object_type == Type::Basic(BasicType::String)
            && matches!(source_name, "byteLen" | "len" | "isEmpty")
            && requested_trait.is_none()
        {
            if !args.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "method `string.{source_name}` expects 0 arguments, got {}",
                        args.len()
                    ),
                ));
                for arg in args {
                    self.validate_expr(arg);
                }
            }

            return if matches!(source_name, "byteLen" | "len") {
                Type::Basic(BasicType::Usize)
            } else {
                Type::Basic(BasicType::Bool)
            };
        }

        let intrinsic = if requested_trait.is_none() {
            match &object_type {
                Type::Struct(struct_name) => self
                    .structs
                    .get(struct_name)
                    .and_then(|struct_| struct_.methods.get(source_name))
                    .cloned(),
                Type::Enum(enum_name) => self
                    .enums
                    .get(enum_name)
                    .and_then(|enum_| enum_.methods.get(source_name))
                    .cloned(),
                Type::Trait(trait_name) => self
                    .traits
                    .get(trait_name)
                    .and_then(|trait_| trait_.methods.get(source_name))
                    .cloned(),
                _ => None,
            }
        } else {
            None
        };
        let signature = if let Some(trait_name) = requested_trait {
            self.qualified_trait_methods
                .get(&qualified_trait_method_name(
                    trait_name,
                    &object_type.name(),
                    source_name,
                ))
                .cloned()
        } else {
            intrinsic
                .or_else(|| {
                    self.extensions
                        .get(&extension_name(&object_type.name(), name))
                        .cloned()
                })
                .or_else(|| {
                    if matches!(object_type, Type::Trait(_)) {
                        None
                    } else {
                        self.trait_methods
                            .get(&trait_method_name(&object_type.name(), source_name))
                            .cloned()
                    }
                })
        };
        let Some(signature) = signature else {
            if matches!(object_type, Type::Basic(_)) {
                self.unsupported(expr.span, "methods on basic values are not implemented yet");
            } else {
                let target = match &object_type {
                    Type::Struct(name) => format!("struct `{name}`"),
                    Type::Enum(name) => format!("enum `{name}`"),
                    Type::Trait(name) => format!("trait `{name}`"),
                    _ => format!("type `{}`", object_type.name()),
                };
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown method `{source_name}` for {target}"),
                ));
            }

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Unknown;
        };
        let qualified_name = format!("{}.{source_name}", object_type.name());

        if signature.mutable_self && !self.expr_has_mutable_capability(object) {
            let message = if let Some(binding_name) = mutable_member_root(object) {
                if let Some(binding) = self.lookup(binding_name)
                    && !binding.mutable
                {
                    match binding.origin {
                        BindingOrigin::MatchPayload {
                            enum_name,
                            variant,
                            mutable_available: true,
                        } => {
                            format!(
                                "cannot call mutable function `{qualified_name}` through immutable match payload `{binding_name}`; bind the payload as mutable with `{enum_name}.{variant}(mut {binding_name})`"
                            )
                        }
                        BindingOrigin::MatchPayload {
                            enum_name,
                            variant,
                            mutable_available: false,
                        } => {
                            format!(
                                "cannot call mutable function `{qualified_name}` through immutable match payload `{binding_name}`; `{enum_name}.{variant}(mut {binding_name})` requires matching a mutable value"
                            )
                        }
                        BindingOrigin::Local => {
                            format!(
                                "cannot call mutable function `{qualified_name}` through immutable binding `{binding_name}`; declare it with `let mut {binding_name}` or call the function on a mutable clone"
                            )
                        }
                    }
                } else {
                    format!(
                        "mutable function `{qualified_name}` requires a mutable receiver; bind the value with `let mut` or call the function on a mutable clone"
                    )
                }
            } else {
                format!(
                    "mutable function `{qualified_name}` requires a mutable receiver; bind the value with `let mut` or call the function on a mutable clone"
                )
            };
            self.diagnostics
                .push(Diagnostic::error(object.span, message));
        }

        if args.len() != signature.params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "method `{qualified_name}` expects {} arguments, got {}",
                    signature.params.len(),
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return signature.return_type;
        }

        for (arg, param) in args.iter().zip(signature.params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_.clone(), arg_type);

            if param.mutable
                && self.requires_mutable_capability(&param.type_)
                && !self.expr_has_mutable_capability(arg)
            {
                self.diagnostics.push(Diagnostic::error(
                    arg.span,
                    format!(
                        "method `{qualified_name}` requires a mutable argument; use `.clone()` to pass an independent mutable object"
                    ),
                ));
            }
        }

        signature.return_type
    }

    fn validate_lambda(
        &mut self,
        span: Span,
        function: &FunctionDecl,
        expected_type: Option<Type>,
    ) -> Type {
        let expected_function = match expected_type {
            Some(Type::Function {
                params,
                return_type,
            }) => Some((params, *return_type)),
            Some(Type::Unknown) | None => None,
            Some(type_) => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("expected function type for lambda, got `{}`", type_.name()),
                ));
                None
            }
        };

        let expected_params = expected_function
            .as_ref()
            .map(|(params, _)| params.as_slice())
            .unwrap_or(&[]);

        if !expected_params.is_empty() && function.params.len() != expected_params.len() {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "lambda expects {} parameters from context, got {}",
                    expected_params.len(),
                    function.params.len()
                ),
            ));
        }

        self.push_scope();
        let mut params = Vec::new();
        for (index, param) in function.params.iter().enumerate() {
            let expected_param = expected_params.get(index);
            let type_ = if let Some(type_ref) = &param.type_ref {
                self.validate_type(type_ref)
            } else if let Some(expected_param) = expected_param {
                expected_param.type_.clone()
            } else {
                self.diagnostics.push(Diagnostic::error(
                    param.span,
                    "lambda parameters must include type annotations when no function type context is available",
                ));
                Type::Unknown
            };

            if let Some(expected_param) = expected_param {
                self.report_type_mismatch(param.span, expected_param.type_.clone(), type_.clone());
                if expected_param.mutable != param.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "lambda parameter mutability does not match function type context",
                    ));
                }
            }

            self.define(&param.name, param.mutable, type_.clone());
            params.push(FunctionTypeParam {
                type_,
                mutable: param.mutable,
            });
        }

        let expected_return = expected_function
            .as_ref()
            .map(|(_, return_type)| return_type.clone());
        let annotated_return = function
            .return_type
            .as_ref()
            .map(|type_ref| self.validate_type(type_ref));
        let return_context = annotated_return
            .clone()
            .or_else(|| expected_return.clone())
            .unwrap_or(Type::Unknown);
        self.return_types.push(return_context.clone());

        let return_type = match &function.body {
            FunctionBody::Expr(expr) => {
                let value_type =
                    self.validate_expr_with_context(expr, Some(return_context.clone()));
                if !matches!(return_context, Type::Unknown) {
                    self.report_type_mismatch(expr.span, return_context.clone(), value_type);
                    return_context
                } else {
                    value_type
                }
            }
            FunctionBody::Block(block) => {
                self.validate_block(block);
                annotated_return
                    .or(expected_return)
                    .unwrap_or(Type::Unknown)
            }
        };

        self.return_types.pop();
        self.pop_scope();

        Type::Function {
            params,
            return_type: Box::new(return_type),
        }
    }

    fn validate_clone(&mut self, span: Span, object: &Expr, args: &[Expr]) -> Type {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("`.clone()` expects no arguments, got {}", args.len()),
            ));
            for arg in args {
                self.validate_expr(arg);
            }
        }

        let object_type = self.validate_expr(object);
        if matches!(
            object_type,
            Type::Struct(_) | Type::Basic(BasicType::String)
        ) {
            object_type
        } else if matches!(object_type, Type::Unknown) {
            Type::Unknown
        } else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "`.clone()` is only supported for struct and string values, got `{}`",
                    object_type.name()
                ),
            ));
            Type::Unknown
        }
    }

    fn validate_variant_call(
        &mut self,
        expr: &Expr,
        enum_name: &str,
        variant_name: &str,
        args: &[Expr],
    ) -> Type {
        let Some(variant) = self
            .enums
            .get(enum_name)
            .and_then(|enum_| enum_.variants.get(variant_name))
            .cloned()
        else {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!("unknown variant `{enum_name}.{variant_name}`"),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Unknown;
        };
        let expected_count = usize::from(variant.is_some());

        if args.len() != expected_count {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "enum variant `{enum_name}.{variant_name}` expects {expected_count} arguments, got {}",
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Enum(enum_name.to_string());
        }

        if let Some(expected_type) = variant {
            let arg_type = self.validate_expr_with_context(&args[0], Some(expected_type.clone()));
            self.report_type_mismatch(args[0].span, expected_type, arg_type);
        }

        Type::Enum(enum_name.to_string())
    }

    fn validate_unit_variant(&mut self, span: Span, enum_name: &str, variant_name: &str) -> Type {
        let Some(variant) = self
            .enums
            .get(enum_name)
            .and_then(|enum_| enum_.variants.get(variant_name))
        else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("unknown variant `{enum_name}.{variant_name}`"),
            ));
            return Type::Unknown;
        };

        if variant.is_some() {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("enum variant `{enum_name}.{variant_name}` requires a payload"),
            ));
            return Type::Unknown;
        }

        Type::Enum(enum_name.to_string())
    }

}
