impl Monomorphizer {
    fn infer_expr_type(&self, expr: &Expr) -> Option<TypeRef> {
        if let Some(type_ref) = self.inferred_expr_types.get(&expr.span) {
            return Some(type_ref.clone());
        }
        let inferred = |name: &str| TypeRef {
            name: name.to_string(),
            args: Vec::new(),
            bindings: Vec::new(),
            function: None,
            span: expr.span,
        };
        match &expr.kind {
            ExprKind::Identifier(name) => self.lookup_local_type(name),
            ExprKind::Number(value) => {
                Some(inferred(if crate::ast::number_literal_is_float(value) {
                    "f64"
                } else {
                    "i32"
                }))
            }
            ExprKind::String(_) => Some(inferred("string")),
            ExprKind::Char(_) => Some(inferred("char")),
            ExprKind::Bool(_) => Some(inferred("bool")),
            ExprKind::StructInit { name, args, fields } => {
                if name == "Self"
                    && args.is_empty()
                    && let Some(type_ref) = self.lookup_local_type("Self")
                {
                    return Some(type_ref);
                }
                if self.struct_templates.contains_key(name) {
                    let args = if args.is_empty() {
                        self.infer_struct_type_arguments(name, fields).ok()?
                    } else {
                        args.clone()
                    };
                    return Some(TypeRef {
                        name: name.clone(),
                        args,
                        bindings: Vec::new(),
                        function: None,
                        span: expr.span,
                    });
                }
                if args.is_empty() {
                    Some(self.expanded_type(&inferred(name)))
                } else {
                    Some(TypeRef {
                        name: name.clone(),
                        args: args.clone(),
                        bindings: Vec::new(),
                        function: None,
                        span: expr.span,
                    })
                }
            }
            ExprKind::Range { inclusive, .. } => {
                let source_name = if *inclusive {
                    "RangeInclusive"
                } else {
                    "Range"
                };
                self.concrete_structs
                    .iter()
                    .find(|name| {
                        *name == source_name || name.ends_with(&format!("::{source_name}"))
                    })
                    .map(|name| TypeRef {
                        name: name.clone(),
                        args: Vec::new(),
                        bindings: Vec::new(),
                        function: None,
                        span: expr.span,
                    })
            }
            ExprKind::GenericType { name, args } => {
                if name == "Self"
                    && args.is_empty()
                    && let Some(type_ref) = self.lookup_local_type("Self")
                {
                    return Some(type_ref);
                }
                if args.is_empty() {
                    Some(self.expanded_type(&inferred(name)))
                } else {
                    Some(TypeRef {
                        name: name.clone(),
                        args: args.clone(),
                        bindings: Vec::new(),
                        function: None,
                        span: expr.span,
                    })
                }
            }
            ExprKind::Member { object, name } => {
                if let ExprKind::GenericType {
                    name: enum_name,
                    args,
                } = &object.kind
                    && self.enum_templates.contains_key(enum_name)
                    && self.enum_templates[enum_name]
                        .variants
                        .iter()
                        .any(|variant| variant.name == *name)
                {
                    return Some(TypeRef {
                        name: enum_name.clone(),
                        args: args.clone(),
                        bindings: Vec::new(),
                        function: None,
                        span: expr.span,
                    });
                }
                if let ExprKind::Identifier(enum_name) = &object.kind
                    && self.enum_variant_payload(enum_name, name).is_some()
                {
                    return Some(inferred(enum_name));
                }
                let object_type = self.infer_expr_type(object)?;
                self.generic_member_type(&object_type, name, false)
            }
            ExprKind::GenericMember { object, name, args } => {
                let receiver = self.infer_expr_type(object)?;
                let (_, mut substitutions, function) =
                    self.method_template(&receiver, name, false)?;
                substitutions.extend(
                    function
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned()),
                );
                let return_type = function.return_type.as_ref()?;
                Some(substitute_type(return_type, &substitutions))
            }
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Member { object, name } = &callee.kind {
                    match &object.kind {
                        ExprKind::Identifier(enum_name)
                            if self.enum_templates.contains_key(enum_name)
                                && self.lookup_local_type(enum_name).is_none() =>
                        {
                            let ExprKind::Call { args, .. } = &expr.kind else {
                                unreachable!("call expression was matched above")
                            };
                            if let Ok(type_args) =
                                self.infer_enum_type_arguments(enum_name, name, args)
                            {
                                return Some(TypeRef {
                                    name: enum_name.clone(),
                                    args: type_args,
                                    bindings: Vec::new(),
                                    function: None,
                                    span: expr.span,
                                });
                            }
                        }
                        ExprKind::GenericType {
                            name: enum_name,
                            args,
                        } if self.enum_templates.contains_key(enum_name) => {
                            return Some(TypeRef {
                                name: enum_name.clone(),
                                args: args.clone(),
                                bindings: Vec::new(),
                                function: None,
                                span: expr.span,
                            });
                        }
                        _ => {}
                    }
                }
                if let ExprKind::Identifier(name) = &callee.kind {
                    if let Some(template_return) = self.generic_function_returns.get(name)
                        && let ExprKind::Call { args, .. } = &expr.kind
                        && let Ok(type_args) = self.infer_function_type_arguments(name, args, None)
                    {
                        let template = &self.function_templates[name];
                        let substitutions = template
                            .type_params
                            .iter()
                            .cloned()
                            .zip(type_args)
                            .collect::<HashMap<_, _>>();
                        return Some(substitute_type(template_return, &substitutions));
                    }
                    if let Some(return_type) = self.function_returns.get(name) {
                        return Some(return_type.clone());
                    }
                }
                if let ExprKind::GenericType { name, args } = &callee.kind
                    && let Some(template_return) = self.generic_function_returns.get(name)
                {
                    let template = &self.function_templates[name];
                    let substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    return Some(substitute_type(template_return, &substitutions));
                }
                if let ExprKind::GenericMember { object, name, args } = &callee.kind {
                    let receiver = self.infer_expr_type(object)?;
                    let (_, mut substitutions, function) =
                        self.method_template(&receiver, name, false)?;
                    substitutions.extend(
                        function
                            .type_params
                            .iter()
                            .cloned()
                            .zip(args.iter().cloned()),
                    );
                    let return_type = function.return_type.as_ref()?;
                    return Some(substitute_type(return_type, &substitutions));
                }
                let ExprKind::Member { object, name } = &callee.kind else {
                    return None;
                };
                if name == "clone" {
                    return self.infer_expr_type(object);
                }
                if let ExprKind::Identifier(type_name) = &object.kind {
                    if self.enum_variant_payload(type_name, name).is_some() {
                        return Some(inferred(type_name));
                    }
                    let type_ref = if type_name == "Self" {
                        self.lookup_local_type("Self")
                    } else {
                        Some(inferred(type_name))
                    };
                    if let Some(type_ref) = type_ref
                        && self.specializations.contains_key(&type_ref.name)
                    {
                        return self.generic_member_type(&type_ref, name, true);
                    }
                }
                let object_type = self.infer_expr_type(object)?;
                self.generic_trait_member_type(&object_type, name)
                    .or_else(|| self.generic_member_type(&object_type, name, false))
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.infer_expr_type(operand)
            }
            ExprKind::Cast { type_ref, .. } => Some(self.expanded_type(type_ref)),
            ExprKind::Binary { left, right, .. } => {
                let left = self.infer_expr_type(left)?;
                let right = self.infer_expr_type(right)?;
                (type_name(&left) == type_name(&right)).then_some(left)
            }
            ExprKind::Match { branches, .. } => branches.iter().find_map(|branch| {
                let MatchBranchBody::Expr(expr) = &branch.body else {
                    return None;
                };
                self.infer_expr_type(expr)
            }),
            ExprKind::CollectionLiteral { collection, .. } => Some(self.expanded_type(collection)),
            ExprKind::Array(_) | ExprKind::Lambda(_) | ExprKind::Missing => None,
        }
    }

    fn generic_member_type(
        &self,
        receiver: &TypeRef,
        member_name: &str,
        static_: bool,
    ) -> Option<TypeRef> {
        if !receiver.args.is_empty() {
            let concrete_name = specialized_name(&receiver.name, &receiver.args);
            if let Some(return_type) =
                self.member_returns
                    .get(&(concrete_name, member_name.to_string(), static_))
            {
                return Some(return_type.clone());
            }
        }
        if let Some(return_type) =
            self.member_returns
                .get(&(receiver.name.clone(), member_name.to_string(), static_))
        {
            return Some(return_type.clone());
        }
        let receiver = self.expanded_type(receiver);
        let (type_params, members, allow_fields) =
            if let Some(template) = self.struct_templates.get(&receiver.name) {
                (&template.type_params, &template.members, true)
            } else if let Some(template) = self.enum_templates.get(&receiver.name) {
                (&template.type_params, &template.members, false)
            } else {
                return None;
            };
        let substitutions = type_params
            .iter()
            .cloned()
            .zip(receiver.args.iter().cloned())
            .collect::<HashMap<_, _>>();
        let return_type = members.iter().find_map(|member| {
            let function = match member {
                StructMember::Method(function) if !static_ => function,
                StructMember::StaticMethod(function) if static_ => function,
                StructMember::Field(field)
                    if allow_fields && !static_ && field.name == member_name =>
                {
                    return Some(field.type_ref.clone());
                }
                _ => return None,
            };
            (function.name.as_deref() == Some(member_name))
                .then(|| function.return_type.clone())
                .flatten()
        })?;
        if return_type.name == "Self" && return_type.args.is_empty() {
            return Some(receiver);
        }
        Some(substitute_type(&return_type, &substitutions))
    }

    fn generic_trait_member_type(&self, receiver: &TypeRef, member_name: &str) -> Option<TypeRef> {
        let receiver = self.expanded_type(receiver);
        let trait_ = self.trait_templates.get(&receiver.name)?;
        let return_type = trait_
            .methods
            .iter()
            .find(|method| method.name == member_name && !method.static_)?
            .return_type
            .as_ref()?;
        let mut substitutions = trait_
            .type_params
            .iter()
            .cloned()
            .zip(receiver.args.iter().cloned())
            .collect::<HashMap<_, _>>();
        substitutions.insert("Self".to_string(), receiver);

        Some(substitute_type(return_type, &substitutions))
    }

    fn infer_type_expression_ref(&self, expr: &Expr) -> Option<TypeRef> {
        match &expr.kind {
            ExprKind::Identifier(name)
                if self.concrete_structs.contains(name)
                    || self.struct_templates.contains_key(name) =>
            {
                Some(TypeRef {
                    name: name.clone(),
                    args: Vec::new(),
                    bindings: Vec::new(),
                    function: None,
                    span: expr.span,
                })
            }
            ExprKind::GenericType { name, args } if self.struct_templates.contains_key(name) => {
                Some(TypeRef {
                    name: name.clone(),
                    args: args.clone(),
                    bindings: Vec::new(),
                    function: None,
                    span: expr.span,
                })
            }
            _ => None,
        }
    }

    fn expanded_type(&self, type_ref: &TypeRef) -> TypeRef {
        if type_ref.args.is_empty()
            && let Some((name, args)) = self.specializations.get(&type_ref.name)
        {
            return TypeRef {
                name: name.clone(),
                args: args.clone(),
                bindings: type_ref.bindings.clone(),
                function: None,
                span: type_ref.span,
            };
        }
        type_ref.clone()
    }

    fn enum_variant_payload(&self, enum_name: &str, variant_name: &str) -> Option<Option<TypeRef>> {
        if let Some(enum_) = self.concrete_enums.get(enum_name) {
            return enum_
                .variants
                .iter()
                .find(|variant| variant.name == variant_name)
                .map(|variant| variant.payload.clone());
        }
        let (generic_name, args) = self.specializations.get(enum_name)?;
        let template = self.enum_templates.get(generic_name)?;
        let substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();
        template
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
            .map(|variant| {
                variant
                    .payload
                    .as_ref()
                    .map(|payload| substitute_type(payload, &substitutions))
            })
    }

    fn lookup_local_type(&self, name: &str) -> Option<TypeRef> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

}
