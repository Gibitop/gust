impl Monomorphizer {
    fn run(mut self, program: &Program) -> Result<Program, Vec<Diagnostic>> {
        self.validate_associated_types(program);
        self.infer_generic_function_returns();
        self.infer_generic_method_returns();
        self.validate_templates();
        self.validate_impl_coherence(program);

        let mut items = Vec::new();
        for item in &program.items {
            if matches!(item, Item::Struct(item) if !item.type_params.is_empty())
                || matches!(item, Item::Enum(item) if !item.type_params.is_empty())
                || matches!(item, Item::Trait(item) if !item.type_params.is_empty())
                || matches!(item, Item::Trait(item) if trait_requires_associated_bindings(item))
                || matches!(item, Item::Function(item) if !item.type_params.is_empty())
                || matches!(item, Item::Impl(item) if !item.type_params.is_empty())
                || matches!(item, Item::Extension(item) if self.is_generic_extension_template(item))
            {
                continue;
            }

            let mut item = item.clone();
            self.rewrite_item(&mut item, &HashMap::new());
            items.push(item);
        }

        loop {
            self.drain_pending(&mut items);
            if !self.instantiate_generic_impl_templates(&mut items) {
                break;
            }
        }
        self.validate_bound_checks(&items);

        prune_unused_generic_methods(&mut items, &self.emitted);
        prune_generic_method_templates(&mut items);

        if self.diagnostics.is_empty() {
            Ok(Program { items })
        } else {
            Err(self.diagnostics)
        }
    }

    fn drain_pending(&mut self, items: &mut Vec<Item>) {
        while let Some(pending) = self.pending.pop_front() {
            match pending {
                PendingSpecialization::Struct(name, args) => {
                    let specialized_name = specialized_name(&name, &args);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
                    let Some(template) = self.struct_templates.get(&name).cloned() else {
                        continue;
                    };
                    let substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    let mut specialized = template;
                    specialized.name = specialized_name;
                    specialized.type_params.clear();
                    specialized.type_param_bounds.clear();
                    self.self_types.push(TypeRef {
                        name: specialized.name.clone(),
                        args: Vec::new(),
                        bindings: Vec::new(),
                        function: None,
                        span: specialized.span,
                    });
                    for member in &mut specialized.members {
                        match member {
                            StructMember::Field(field) => {
                                self.rewrite_type(&mut field.type_ref, &substitutions);
                            }
                            StructMember::Method(function)
                            | StructMember::StaticMethod(function) => {
                                if function.type_params.is_empty() {
                                    self.rewrite_function(function, &substitutions);
                                }
                            }
                        }
                    }
                    self.infer_specialized_member_returns(
                        &specialized.name,
                        &mut specialized.members,
                    );
                    self.self_types.pop();
                    items.push(Item::Struct(specialized));
                }
                PendingSpecialization::Enum(name, args) => {
                    let specialized_name = specialized_name(&name, &args);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
                    let Some(template) = self.enum_templates.get(&name).cloned() else {
                        continue;
                    };
                    let substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    let mut specialized = template;
                    specialized.name = specialized_name;
                    specialized.type_params.clear();
                    specialized.type_param_bounds.clear();
                    self.self_types.push(TypeRef {
                        name: specialized.name.clone(),
                        args: Vec::new(),
                        bindings: Vec::new(),
                        function: None,
                        span: specialized.span,
                    });
                    for variant in &mut specialized.variants {
                        if let Some(payload) = &mut variant.payload {
                            self.rewrite_type(payload, &substitutions);
                        }
                    }
                    for member in &mut specialized.members {
                        match member {
                            StructMember::Method(function)
                            | StructMember::StaticMethod(function) => {
                                if function.type_params.is_empty() {
                                    self.rewrite_function(function, &substitutions);
                                }
                            }
                            StructMember::Field(_) => {}
                        }
                    }
                    self.infer_specialized_member_returns(
                        &specialized.name,
                        &mut specialized.members,
                    );
                    self.self_types.pop();
                    self.concrete_enums
                        .insert(specialized.name.clone(), specialized.clone());
                    items.push(Item::Enum(specialized));
                }
                PendingSpecialization::Trait(name, args, bindings) => {
                    let specialized_name = specialized_trait_name(&name, &args, &bindings);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
                    let Some(template) = self.trait_declarations.get(&name).cloned() else {
                        continue;
                    };
                    let mut substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    substitutions.extend(bindings.iter().map(|binding| {
                        (
                            format!("Self.{}", binding.name),
                            binding.type_ref.clone(),
                        )
                    }));
                    let mut specialized = template;
                    specialized.name = specialized_name;
                    specialized.type_params.clear();
                    specialized.type_param_bounds.clear();
                    for method in &mut specialized.methods {
                        for param in &mut method.params {
                            if let Some(type_ref) = &mut param.type_ref {
                                self.rewrite_type(type_ref, &substitutions);
                            }
                        }
                        if let Some(return_type) = &mut method.return_type {
                            self.rewrite_type(return_type, &substitutions);
                        }
                    }
                    items.push(Item::Trait(specialized));
                }
                PendingSpecialization::Function(name, args) => {
                    let specialized_name = specialized_name(&name, &args);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
                    let Some(template) = self.function_templates.get(&name).cloned() else {
                        continue;
                    };
                    let substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    let mut specialized = template;
                    specialized.name = Some(specialized_name.clone());
                    specialized.type_params.clear();
                    specialized.type_param_bounds.clear();
                    for param in &mut specialized.params {
                        if let Some(type_ref) = &mut param.type_ref {
                            *type_ref = substitute_type(type_ref, &substitutions);
                        }
                    }
                    if let Some(return_type) = &mut specialized.return_type {
                        *return_type = substitute_type(return_type, &substitutions);
                    } else if let Some(return_type) = self.generic_function_returns.get(&name) {
                        specialized.return_type =
                            Some(substitute_type(return_type, &substitutions));
                    }
                    if let Some(return_type) = &specialized.return_type {
                        self.function_returns
                            .insert(specialized_name.clone(), return_type.clone());
                    }
                    self.function_params.insert(
                        specialized_name,
                        specialized
                            .params
                            .iter()
                            .map(|param| param.type_ref.clone())
                            .collect(),
                    );
                    self.rewrite_function(&mut specialized, &substitutions);
                    items.push(Item::Function(specialized));
                }
                PendingSpecialization::Method {
                    receiver,
                    name,
                    static_,
                    args,
                } => {
                    let method_name = specialized_name(&name, &args);
                    let emitted_name = format!("{receiver}.{method_name}");
                    if !self.emitted.insert(emitted_name) {
                        continue;
                    }
                    self.emit_method_specialization(items, &receiver, &name, static_, &args);
                }
                PendingSpecialization::Extension {
                    template_index,
                    receiver,
                    function_args,
                } => {
                    self.emit_extension_specialization(
                        items,
                        template_index,
                        &receiver,
                        &function_args,
                    );
                }
            }
        }
    }

    fn instantiate_generic_impl_templates(&mut self, items: &mut Vec<Item>) -> bool {
        let mut concrete_types = concrete_type_refs(items);
        concrete_types.extend(self.impl_receiver_types.clone());
        let concrete_traits = concrete_trait_refs(items);
        let mut changed = false;

        for template in self.impl_templates.clone() {
            for concrete_type in &concrete_types {
                let mut type_args = Vec::new();
                if let Ok(args) = self.solve_type_arguments(
                    "impl",
                    &template.type_params,
                    vec![(template.type_ref.clone(), concrete_type.clone())],
                ) {
                    type_args.push(args);
                }

                for concrete_trait in &concrete_traits {
                    if let Ok(args) = self.solve_type_arguments(
                        "impl",
                        &template.type_params,
                        vec![
                            (template.type_ref.clone(), concrete_type.clone()),
                            (template.trait_ref.clone(), concrete_trait.clone()),
                        ],
                    ) {
                        type_args.push(args);
                    }
                }

                for args in type_args {
                    let substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    if !self.type_param_bounds_satisfied(
                        &template.type_param_bounds,
                        &substitutions,
                        items,
                    ) {
                        continue;
                    }

                    let mut specialized = template.clone();
                    specialized.type_params.clear();
                    specialized.type_param_bounds.clear();
                    specialized.trait_ref = substitute_type(&template.trait_ref, &substitutions);
                    specialized.type_ref = substitute_type(&template.type_ref, &substitutions);
                    self.rewrite_type(&mut specialized.type_ref, &HashMap::new());
                    for associated_type in &mut specialized.associated_types {
                        associated_type.type_ref =
                            substitute_type(&associated_type.type_ref, &substitutions);
                        self.rewrite_type(&mut associated_type.type_ref, &HashMap::new());
                    }
                    let required_associated_types = self
                        .trait_declarations
                        .get(&specialized.trait_ref.name)
                        .map(trait_required_associated_type_names)
                        .unwrap_or_default();
                    specialized.trait_ref.bindings = specialized
                        .associated_types
                        .iter()
                        .filter(|associated_type| {
                            required_associated_types.contains(&associated_type.name)
                        })
                        .map(|associated_type| crate::ast::AssociatedTypeBinding {
                            name: associated_type.name.clone(),
                            type_ref: associated_type.type_ref.clone(),
                            span: associated_type.span,
                        })
                        .collect();
                    self.rewrite_type(&mut specialized.trait_ref, &HashMap::new());

                    let key = format!(
                        "impl {} for {}",
                        type_name(&specialized.trait_ref),
                        type_name(&specialized.type_ref)
                    );
                    if !self.emitted.insert(key) {
                        continue;
                    }
                    if concrete_impl_exists(items, &specialized.trait_ref, &specialized.type_ref) {
                        continue;
                    }

                    self.self_types.push(specialized.type_ref.clone());
                    let mut impl_substitutions = substitutions.clone();
                    impl_substitutions.extend(specialized.associated_types.iter().map(
                        |associated_type| {
                            (
                                format!("Self.{}", associated_type.name),
                                associated_type.type_ref.clone(),
                            )
                        },
                    ));
                    for member in &mut specialized.methods {
                        self.rewrite_function(&mut member.function, &impl_substitutions);
                    }
                    self.self_types.pop();

                    items.push(Item::Impl(specialized));
                    changed = true;
                }
            }
        }

        changed
    }

}
