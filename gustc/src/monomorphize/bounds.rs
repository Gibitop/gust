impl Monomorphizer {
    fn type_param_bounds_satisfied(
        &mut self,
        bounds: &[TypeParamBound],
        substitutions: &HashMap<String, TypeRef>,
        items: &[Item],
    ) -> bool {
        bounds.iter().all(|bound| {
            let Some(type_ref) = substitutions.get(&bound.param) else {
                return true;
            };
            let mut trait_ref = substitute_type(&bound.trait_ref, substitutions);
            self.rewrite_type(&mut trait_ref, &HashMap::new());
            concrete_impl_exists(items, &trait_ref, type_ref)
        })
    }

    fn record_type_param_bound_checks(
        &mut self,
        owner: String,
        params: &[String],
        bounds: &[TypeParamBound],
        args: &[TypeRef],
        span: crate::span::Span,
    ) {
        if bounds.is_empty() {
            return;
        }
        let substitutions = params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for bound in bounds {
            let Some(type_ref) = substitutions.get(&bound.param).cloned() else {
                continue;
            };
            let mut trait_ref = substitute_type(&bound.trait_ref, &substitutions);
            self.rewrite_type(&mut trait_ref, &HashMap::new());
            self.request_impl(&trait_ref, &type_ref);
            self.bound_checks.push(BoundCheck {
                owner: owner.clone(),
                type_ref,
                trait_ref,
                span,
            });
        }
    }

    fn validate_bound_checks(&mut self, items: &[Item]) {
        let mut reported = HashSet::new();
        for check in &self.bound_checks {
            if concrete_impl_exists(items, &check.trait_ref, &check.type_ref) {
                continue;
            }
            let key = (
                check.owner.clone(),
                type_name(&check.type_ref),
                type_name(&check.trait_ref),
                check.span,
            );
            if !reported.insert(key) {
                continue;
            }
            self.diagnostics.push(Diagnostic::error(
                check.span,
                format!(
                    "type `{}` does not satisfy bound `{}: {}` required by {}",
                    type_name(&check.type_ref),
                    type_name(&check.type_ref),
                    type_name(&check.trait_ref),
                    check.owner
                ),
            ));
        }
    }

    fn validate_templates(&mut self) {
        for template in self.struct_templates.values().cloned().collect::<Vec<_>>() {
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "duplicate type parameter `{name}` in struct `{}`",
                            template.name
                        ),
                    ));
                }
            }
            self.validate_method_type_params(
                &template.name,
                "struct",
                &template.type_params,
                &template.members,
            );
        }
        for template in self
            .concrete_struct_defs
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            self.validate_method_type_params(
                &template.name,
                "struct",
                &template.type_params,
                &template.members,
            );
        }
        for template in self.enum_templates.values().cloned().collect::<Vec<_>>() {
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "duplicate type parameter `{name}` in enum `{}`",
                            template.name
                        ),
                    ));
                }
            }
            self.validate_method_type_params(
                &template.name,
                "enum",
                &template.type_params,
                &template.members,
            );
        }
        for template in self.concrete_enums.values().cloned().collect::<Vec<_>>() {
            self.validate_method_type_params(
                &template.name,
                "enum",
                &template.type_params,
                &template.members,
            );
        }
        for template in self.trait_templates.values() {
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "duplicate type parameter `{name}` in trait `{}`",
                            template.name
                        ),
                    ));
                }
            }
            let used = template
                .methods
                .iter()
                .flat_map(|method| {
                    method
                        .params
                        .iter()
                        .filter_map(|param| param.type_ref.as_ref())
                        .chain(method.return_type.as_ref())
                })
                .flat_map(type_names)
                .collect::<HashSet<_>>();
            for name in &template.type_params {
                if !used.contains(name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "unused type parameter `{name}` in trait `{}`",
                            template.name
                        ),
                    ));
                }
            }
        }
        for template in self.function_templates.values() {
            let function_name = template.name.as_deref().unwrap_or("<anonymous>");
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!("duplicate type parameter `{name}` in function `{function_name}`"),
                    ));
                }
            }
            let used = template
                .params
                .iter()
                .filter_map(|param| param.type_ref.as_ref())
                .chain(self.generic_function_returns.get(function_name))
                .flat_map(type_names)
                .collect::<HashSet<_>>();
            for name in &template.type_params {
                if !used.contains(name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!("unused type parameter `{name}` in function `{function_name}`"),
                    ));
                }
            }
        }
        for template in &self.impl_templates {
            let impl_name = format!(
                "{} for {}",
                type_name(&template.trait_ref),
                type_name(&template.type_ref)
            );
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!("duplicate type parameter `{name}` in impl `{impl_name}`"),
                    ));
                }
            }
            let used = type_names(&template.trait_ref)
                .into_iter()
                .chain(type_names(&template.type_ref))
                .collect::<HashSet<_>>();
            for name in &template.type_params {
                if !used.contains(name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!("unused type parameter `{name}` in impl `{impl_name}`"),
                    ));
                }
            }
        }
    }

    fn validate_method_type_params(
        &mut self,
        owner_name: &str,
        owner_kind: &str,
        owner_type_params: &[String],
        members: &[StructMember],
    ) {
        let owner_params = owner_type_params.iter().cloned().collect::<HashSet<_>>();
        for member in members {
            let function = match member {
                StructMember::Method(function) | StructMember::StaticMethod(function) => function,
                StructMember::Field(_) => continue,
            };
            if function.type_params.is_empty() {
                continue;
            }
            let function_name = function.name.as_deref().unwrap_or("<anonymous>");
            let mut names = HashSet::new();
            for name in &function.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        function.span,
                        format!("duplicate type parameter `{name}` in method `{function_name}`"),
                    ));
                }
                if owner_params.contains(name) {
                    self.diagnostics.push(Diagnostic::error(
                        function.span,
                        format!(
                            "type parameter `{name}` in method `{function_name}` conflicts with {owner_kind} `{owner_name}`",
                        ),
                    ));
                }
            }
            let used = function
                .params
                .iter()
                .filter_map(|param| param.type_ref.as_ref())
                .chain(function.return_type.as_ref().or_else(|| {
                    self.generic_method_returns.get(&(
                        owner_name.to_string(),
                        function_name.to_string(),
                        matches!(member, StructMember::StaticMethod(_)),
                    ))
                }))
                .flat_map(type_names)
                .collect::<HashSet<_>>();
            for name in &function.type_params {
                if !used.contains(name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        function.span,
                        format!("unused type parameter `{name}` in method `{function_name}`"),
                    ));
                }
            }
        }
    }

}
