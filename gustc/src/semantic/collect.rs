impl Analyzer {
    fn new() -> Self {
        let values = HashSet::from(["io".to_string()]);
        let types = HashSet::from(["void".to_string(), "ArrayList".to_string()]);

        Self {
            diagnostics: Vec::new(),
            values,
            types,
            structs: HashMap::new(),
            enums: HashMap::new(),
            traits: HashMap::new(),
            functions: HashMap::new(),
            extensions: HashMap::new(),
            static_extensions: HashMap::new(),
            trait_methods: HashMap::new(),
            qualified_trait_methods: HashMap::new(),
            static_trait_methods: HashMap::new(),
            qualified_static_trait_methods: HashMap::new(),
            trait_impls: HashSet::new(),
            imported_namespaces: HashSet::new(),
            unsupported_features: HashSet::new(),
            scopes: Vec::new(),
            return_types: Vec::new(),
            self_types: Vec::new(),
            direct_struct_methods: Vec::new(),
            loop_depth: 0,
        }
    }

    fn collect_top_level(&mut self, program: &Program) {
        let mut names: HashMap<String, Span> = HashMap::new();
        let mut main_count = 0;

        for item in &program.items {
            match item {
                Item::Import(item) => {
                    if let Some(namespace) = &item.namespace {
                        self.values.insert(namespace.name.clone());
                        self.imported_namespaces.insert(namespace.name.clone());
                        self.insert_top_level(&mut names, &namespace.name, namespace.span);
                    }
                    for import in &item.names {
                        let name = import.alias.as_ref().unwrap_or(&import.name);
                        self.values.insert(name.clone());
                        self.types.insert(name.clone());
                        self.insert_top_level(&mut names, name, import.span);
                    }
                }
                Item::Enum(item) => {
                    self.types.insert(item.name.clone());
                    self.enums.insert(
                        item.name.clone(),
                        EnumDefinition {
                            variants: HashMap::new(),
                            methods: HashMap::new(),
                            static_methods: HashMap::new(),
                        },
                    );
                    self.insert_top_level(&mut names, &item.name, item.span);
                }
                Item::Struct(item) => {
                    self.values.insert(item.name.clone());
                    self.types.insert(item.name.clone());
                    self.structs.insert(
                        item.name.clone(),
                        StructDefinition {
                            fields: HashMap::new(),
                            methods: HashMap::new(),
                            static_methods: HashMap::new(),
                        },
                    );
                    self.insert_top_level(&mut names, &item.name, item.span);
                }
                Item::Trait(item) => {
                    self.types.insert(item.name.clone());
                    self.traits.insert(
                        item.name.clone(),
                        TraitDefinition {
                            methods: HashMap::new(),
                            static_methods: HashMap::new(),
                        },
                    );
                    self.insert_top_level(&mut names, &item.name, item.span);
                }
                Item::Function(item) => {
                    if let Some(name) = &item.name {
                        if name == "main" {
                            main_count += 1;
                        }

                        self.values.insert(name.clone());
                        self.insert_top_level(&mut names, name, item.span);
                    }
                }
                Item::Impl(_) => {}
                Item::Extension(_) => {}
            }
        }

        for item in &program.items {
            match item {
                Item::Struct(item) => self.collect_struct_definition(item),
                Item::Enum(item) => self.collect_enum_definition(item),
                Item::Trait(item) => self.collect_trait_definition(item),
                Item::Function(item) => {
                    if let Some(name) = &item.name {
                        self.functions.insert(
                            name.clone(),
                            FunctionSignature {
                                params: item
                                    .params
                                    .iter()
                                    .map(|param| ParamSignature {
                                        type_: self
                                            .type_ref_without_diagnostics(param.type_ref.as_ref()),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_without_diagnostics(item.return_type.as_ref()),
                                mutable_self: false,
                            },
                        );
                    }
                }
                Item::Impl(_) => {}
                Item::Extension(item) => self.collect_extension_definition(item),
                Item::Import(_) => {}
            }
        }

        for item in &program.items {
            if let Item::Impl(item) = item {
                self.collect_impl_definition(item);
            }
        }

        if main_count == 0 {
            let span = program
                .items
                .first()
                .map_or_else(|| Span::new(0, 0), Item::span);
            self.diagnostics
                .push(Diagnostic::error(span, "missing `main` function"));
        } else if main_count > 1 {
            self.diagnostics.push(Diagnostic::error(
                Span::new(0, 0),
                "expected exactly one `main` function",
            ));
        }
    }

    fn collect_struct_definition(&mut self, item: &StructDecl) {
        let mut fields = HashMap::new();
        let mut methods = HashMap::new();
        let mut static_methods = HashMap::new();
        let self_type = Type::Struct(item.name.clone());

        for member in &item.members {
            match member {
                StructMember::Field(field) => {
                    if fields
                        .insert(
                            field.name.clone(),
                            StructField {
                                type_: self.type_ref_without_diagnostics(Some(&field.type_ref)),
                                internal: field.internal,
                            },
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            field.span,
                            format!("duplicate field `{}` in struct `{}`", field.name, item.name),
                        ));
                    }
                }
                StructMember::Method(method) => {
                    let Some(name) = &method.name else {
                        continue;
                    };

                    if name == "clone" {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            "method name `clone` is reserved for the built-in deep clone operation",
                        ));
                    }

                    if methods
                        .insert(
                            name.clone(),
                            FunctionSignature {
                                params: method
                                    .params
                                    .iter()
                                    .filter(|param| !is_self_param(param))
                                    .map(|param| ParamSignature {
                                        type_: self.type_ref_in_context(
                                            param.type_ref.as_ref(),
                                            &self_type,
                                        ),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_in_context(method.return_type.as_ref(), &self_type),
                                mutable_self: has_mutable_receiver(method),
                            },
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            format!("duplicate method `{name}` in struct `{}`", item.name),
                        ));
                    }
                }
                StructMember::StaticMethod(method) => {
                    let Some(name) = &method.name else {
                        continue;
                    };

                    if name == "clone" {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            "static function name `clone` is reserved for the built-in deep clone operation",
                        ));
                    }

                    if static_methods
                        .insert(
                            name.clone(),
                            FunctionSignature {
                                params: method
                                    .params
                                    .iter()
                                    .map(|param| ParamSignature {
                                        type_: self.type_ref_in_context(
                                            param.type_ref.as_ref(),
                                            &self_type,
                                        ),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_in_context(method.return_type.as_ref(), &self_type),
                                mutable_self: false,
                            },
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            format!(
                                "duplicate static function `{name}` in struct `{}`",
                                item.name
                            ),
                        ));
                    }
                }
            }
        }

        self.structs.insert(
            item.name.clone(),
            StructDefinition {
                fields,
                methods,
                static_methods,
            },
        );
    }

    fn collect_enum_definition(&mut self, item: &crate::ast::EnumDecl) {
        let mut variants = HashMap::new();
        let mut methods = HashMap::new();
        let mut static_methods = HashMap::new();
        let self_type = Type::Enum(item.name.clone());

        for variant in &item.variants {
            let payload = variant
                .payload
                .as_ref()
                .map(|type_ref| self.type_ref_without_diagnostics(Some(type_ref)));

            if variants
                .insert(variant.name.clone(), payload.clone())
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    variant.span,
                    format!(
                        "duplicate variant `{}` in enum `{}`",
                        variant.name, item.name
                    ),
                ));
            }
        }

        for member in &item.members {
            match member {
                StructMember::Method(method) => {
                    let Some(name) = &method.name else {
                        continue;
                    };

                    if name == "clone" {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            "method name `clone` is reserved for the built-in deep clone operation",
                        ));
                    }

                    if methods
                        .insert(
                            name.clone(),
                            FunctionSignature {
                                params: method
                                    .params
                                    .iter()
                                    .filter(|param| !is_self_param(param))
                                    .map(|param| ParamSignature {
                                        type_: self.type_ref_in_context(
                                            param.type_ref.as_ref(),
                                            &self_type,
                                        ),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_in_context(method.return_type.as_ref(), &self_type),
                                mutable_self: has_mutable_receiver(method),
                            },
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            format!("duplicate method `{name}` in enum `{}`", item.name),
                        ));
                    }
                }
                StructMember::StaticMethod(method) => {
                    let Some(name) = &method.name else {
                        continue;
                    };

                    if name == "clone" {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            "static function name `clone` is reserved for the built-in deep clone operation",
                        ));
                    }

                    if static_methods
                        .insert(
                            name.clone(),
                            FunctionSignature {
                                params: method
                                    .params
                                    .iter()
                                    .map(|param| ParamSignature {
                                        type_: self.type_ref_in_context(
                                            param.type_ref.as_ref(),
                                            &self_type,
                                        ),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_in_context(method.return_type.as_ref(), &self_type),
                                mutable_self: false,
                            },
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            format!("duplicate static function `{name}` in enum `{}`", item.name),
                        ));
                    }
                }
                StructMember::Field(_) => {}
            }
        }

        self.enums.insert(
            item.name.clone(),
            EnumDefinition {
                variants,
                methods,
                static_methods,
            },
        );
    }

    fn collect_trait_definition(&mut self, item: &TraitDecl) {
        let mut methods = HashMap::new();
        let mut static_methods = HashMap::new();

        for method in &item.methods {
            if method.name == "clone" {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    "trait method name `clone` is reserved for the built-in deep clone operation",
                ));
            }

            if method.return_type.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "trait method `{}.{}` must include a return type",
                        item.name, method.name
                    ),
                ));
            }

            let target_methods = if method.static_ {
                &mut static_methods
            } else {
                &mut methods
            };

            if target_methods
                .insert(
                    method.name.clone(),
                    self.trait_method_signature(method, method.static_),
                )
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "duplicate {}method `{}` in trait `{}`",
                        if method.static_ { "static " } else { "" },
                        method.name,
                        item.name
                    ),
                ));
            }
        }

        self.traits.insert(
            item.name.clone(),
            TraitDefinition {
                methods,
                static_methods,
            },
        );
    }

    fn collect_impl_definition(&mut self, item: &ImplDecl) {
        let trait_name = item.trait_ref.name.clone();
        let self_type = self.type_ref_without_diagnostics(Some(&item.type_ref));
        let self_type_name = self_type.name();

        let Some(trait_) = self.traits.get(&trait_name).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                item.trait_ref.span,
                format!("unknown trait `{trait_name}`"),
            ));
            return;
        };

        if matches!(
            self_type,
            Type::Unknown | Type::Void | Type::Trait(_) | Type::Function { .. } | Type::Named(_)
        ) {
            return;
        }

        if !self
            .trait_impls
            .insert((trait_name.clone(), self_type_name.clone()))
        {
            self.diagnostics.push(Diagnostic::error(
                item.span,
                format!("duplicate impl of trait `{trait_name}` for type `{self_type_name}`"),
            ));
        }

        let mut impl_methods = HashMap::new();
        let mut static_impl_methods = HashMap::new();
        for member in &item.methods {
            let method = &member.function;
            let Some(name) = &method.name else {
                continue;
            };
            if name == "clone" {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    "trait impl method name `clone` is reserved for the built-in deep clone operation",
                ));
            }

            let expected = if member.static_ {
                trait_.static_methods.get(name)
            } else {
                trait_.methods.get(name)
            }
            .map(|signature| signature_with_self_type(signature, &self_type));

            let signature = FunctionSignature {
                params: method
                    .params
                    .iter()
                    .filter(|param| member.static_ || !is_self_param(param))
                    .map(|param| ParamSignature {
                        type_: self.type_ref_in_context(param.type_ref.as_ref(), &self_type),
                        mutable: param.mutable,
                    })
                    .collect(),
                return_type: method.return_type.as_ref().map_or_else(
                    || {
                        expected
                            .as_ref()
                            .map_or(Type::Unknown, |signature| signature.return_type.clone())
                    },
                    |return_type| self.type_ref_in_context(Some(return_type), &self_type),
                ),
                mutable_self: !member.static_ && has_mutable_receiver(method),
            };

            let target_impl_methods = if member.static_ {
                &mut static_impl_methods
            } else {
                &mut impl_methods
            };
            if target_impl_methods
                .insert(name.clone(), signature.clone())
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "duplicate {}method `{name}` in impl of trait `{trait_name}` for type `{self_type_name}`",
                        if member.static_ { "static " } else { "" },
                    ),
                ));
            }

            let Some(expected) = expected else {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "{}method `{name}` is not declared in trait `{trait_name}`",
                        if member.static_ { "static " } else { "" },
                    ),
                ));
                continue;
            };

            if !signature_contains_associated_projection(&expected)
                && !signatures_match(&expected, &signature)
            {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "method `{name}` does not match trait `{trait_name}` for type `{self_type_name}`"
                    ),
                ));
            }

            let trait_method_name = if member.static_ {
                static_trait_method_name(&self_type_name, name)
            } else {
                trait_method_name(&self_type_name, name)
            };
            let trait_methods = if member.static_ {
                &mut self.static_trait_methods
            } else {
                &mut self.trait_methods
            };
            let (qualified_name, qualified_trait_methods) = if member.static_ {
                (
                    qualified_static_trait_method_name(&trait_name, &self_type_name, name),
                    &mut self.qualified_static_trait_methods,
                )
            } else {
                (
                    qualified_trait_method_name(&trait_name, &self_type_name, name),
                    &mut self.qualified_trait_methods,
                )
            };
            if qualified_trait_methods
                .insert(qualified_name, signature.clone())
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "duplicate trait method `{name}` for type `{self_type_name}` in trait `{trait_name}`"
                    ),
                ));
            }
            let skip_unqualified = trait_has_positional_type_arguments(&trait_name);
            if !skip_unqualified && trait_methods.insert(trait_method_name, signature).is_some() {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "duplicate {}trait method `{name}` for type `{self_type_name}`",
                        if member.static_ { "static " } else { "" },
                    ),
                ));
            }
        }

        for name in trait_.methods.keys() {
            if !impl_methods.contains_key(name) {
                self.diagnostics.push(Diagnostic::error(
                    item.span,
                    format!(
                        "impl of trait `{trait_name}` for type `{self_type_name}` is missing method `{name}`"
                    ),
                ));
            }
        }
        for name in trait_.static_methods.keys() {
            if !static_impl_methods.contains_key(name) {
                self.diagnostics.push(Diagnostic::error(
                    item.span,
                    format!(
                        "impl of trait `{trait_name}` for type `{self_type_name}` is missing static method `{name}`"
                    ),
                ));
            }
        }
    }

    fn trait_method_signature(&self, method: &TraitMethodDecl, static_: bool) -> FunctionSignature {
        FunctionSignature {
            params: method
                .params
                .iter()
                .filter(|param| static_ || !is_self_param(param))
                .map(|param| ParamSignature {
                    type_: self.trait_type_ref_without_diagnostics(param.type_ref.as_ref()),
                    mutable: param.mutable,
                })
                .collect(),
            return_type: self.trait_type_ref_without_diagnostics(method.return_type.as_ref()),
            mutable_self: !static_
                && method
                    .params
                    .iter()
                    .any(|param| is_self_param(param) && param.mutable && param.type_ref.is_none()),
        }
    }

    fn trait_type_ref_without_diagnostics(&self, type_ref: Option<&TypeRef>) -> Type {
        if let Some(type_ref) = type_ref
            && type_ref.name == "Self"
            && type_ref.args.is_empty()
        {
            Type::Named("Self".to_string())
        } else if let Some(type_ref) = type_ref
            && type_ref.name.starts_with("Self.")
        {
            Type::Named(type_ref.name.clone())
        } else {
            self.type_ref_without_diagnostics(type_ref)
        }
    }

    fn collect_extension_definition(&mut self, item: &crate::ast::ExtensionDecl) {
        let Some(name) = &item.function.name else {
            return;
        };
        let self_type = self.type_ref_without_diagnostics(Some(&item.type_ref));

        if name == "clone" {
            self.diagnostics.push(Diagnostic::error(
                item.function.span,
                "extension function name `clone` is reserved for the built-in deep clone operation",
            ));
        }

        let signature = FunctionSignature {
            params: item
                .function
                .params
                .iter()
                .filter(|param| item.static_ || !is_self_param(param))
                .map(|param| ParamSignature {
                    type_: self.type_ref_in_context(param.type_ref.as_ref(), &self_type),
                    mutable: param.mutable,
                })
                .collect(),
            return_type: self.type_ref_in_context(item.function.return_type.as_ref(), &self_type),
            mutable_self: !item.static_ && has_mutable_receiver(&item.function),
        };
        let extensions = if item.static_ {
            &mut self.static_extensions
        } else {
            &mut self.extensions
        };

        if extensions
            .insert(extension_name(&self_type.name(), name), signature)
            .is_some()
        {
            self.diagnostics.push(Diagnostic::error(
                item.function.span,
                format!(
                    "duplicate {}extension function `{}` for type `{}`",
                    if item.static_ { "static " } else { "" },
                    name,
                    item.type_ref.name
                ),
            ));
        }
    }

    fn insert_top_level(&mut self, names: &mut HashMap<String, Span>, name: &str, span: Span) {
        if let Some(previous_span) = names.insert(name.to_string(), span) {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("duplicate top-level name `{name}`"),
            ));
            self.diagnostics.push(Diagnostic::error(
                previous_span,
                format!("first definition of `{name}` is here"),
            ));
        }
    }

}
