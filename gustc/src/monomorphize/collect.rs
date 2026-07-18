impl Monomorphizer {
    fn new(program: &Program) -> Self {
        let struct_templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Struct(item) = item else {
                    return None;
                };
                (!item.type_params.is_empty()).then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let enum_templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Enum(item) = item else {
                    return None;
                };
                (!item.type_params.is_empty()).then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let function_templates: HashMap<String, FunctionDecl> = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(item) = item else {
                    return None;
                };
                (!item.type_params.is_empty())
                    .then(|| item.name.clone().map(|name| (name, item.clone())))
                    .flatten()
            })
            .collect();
        let trait_declarations: HashMap<String, TraitDecl> = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Trait(item) = item else {
                    return None;
                };
                Some((item.name.clone(), item.clone()))
            })
            .collect();
        let trait_templates = trait_declarations
            .values()
            .filter_map(|item| {
                (!item.type_params.is_empty()).then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let impl_templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Impl(item) = item else {
                    return None;
                };
                (!item.type_params.is_empty()).then(|| item.clone())
            })
            .collect();
        let impl_declarations = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Impl(item) = item else {
                    return None;
                };
                Some(item.clone())
            })
            .collect();
        let mut extensions: Vec<crate::ast::ExtensionDecl> = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Extension(item) = item else {
                    return None;
                };
                Some(item.clone())
            })
            .collect();
        let trait_default_extension_start = extensions.len();
        extensions.extend(trait_default_extensions(program));
        let concrete_structs = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Struct(item) = item else {
                    return None;
                };
                item.type_params.is_empty().then(|| item.name.clone())
            })
            .collect();
        let concrete_struct_defs = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Struct(item) = item else {
                    return None;
                };
                item.type_params
                    .is_empty()
                    .then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let concrete_enums = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Enum(item) = item else {
                    return None;
                };
                item.type_params
                    .is_empty()
                    .then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let concrete_traits = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Trait(item) = item else {
                    return None;
                };
                item.type_params.is_empty().then(|| item.name.clone())
            })
            .collect();
        let function_returns = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(function) = item else {
                    return None;
                };
                if !function.type_params.is_empty() {
                    return None;
                }
                Some((
                    function.name.clone()?,
                    function.return_type.as_ref()?.clone(),
                ))
            })
            .collect();
        let function_params = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(function) = item else {
                    return None;
                };
                if !function.type_params.is_empty() {
                    return None;
                }
                Some((
                    function.name.clone()?,
                    function
                        .params
                        .iter()
                        .map(|param| param.type_ref.clone())
                        .collect(),
                ))
            })
            .collect();
        let static_types = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::StaticVar(item) = item else {
                    return None;
                };
                Some((item.name.clone(), item.type_annotation.as_ref()?.clone()))
            })
            .collect();
        let generic_function_returns = function_templates
            .iter()
            .filter_map(|(name, function)| {
                function
                    .return_type
                    .clone()
                    .map(|return_type| (name.clone(), return_type))
            })
            .collect();

        Self {
            struct_templates,
            enum_templates,
            trait_declarations,
            trait_templates,
            impl_declarations,
            impl_templates,
            extensions,
            trait_default_extension_start,
            function_templates,
            concrete_structs,
            concrete_struct_defs,
            concrete_enums,
            concrete_traits,
            pending: VecDeque::new(),
            emitted: HashSet::new(),
            specializations: HashMap::new(),
            trait_specializations: HashMap::new(),
            scopes: Vec::new(),
            return_types: Vec::new(),
            self_types: Vec::new(),
            inferred_returns: Vec::new(),
            member_returns: HashMap::new(),
            function_returns,
            function_params,
            static_types,
            generic_function_returns,
            generic_method_returns: HashMap::new(),
            trait_method_returns: HashMap::new(),
            expected_expr_types: HashMap::new(),
            inferred_expr_types: HashMap::new(),
            impl_receiver_types: Vec::new(),
            bound_checks: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

}

fn trait_default_extensions(program: &Program) -> Vec<crate::ast::ExtensionDecl> {
    program
        .items
        .iter()
        .filter_map(|item| {
            let Item::Trait(trait_) = item else {
                return None;
            };
            Some(
                trait_
                    .methods
                    .iter()
                    .filter_map(|method| trait_default_extension(trait_, method)),
            )
        })
        .flatten()
        .collect()
}

fn trait_default_extension(
    trait_: &TraitDecl,
    method: &crate::ast::TraitMethodDecl,
) -> Option<crate::ast::ExtensionDecl> {
    if method.static_ {
        return None;
    }
    let body = method.body.clone()?;
    let mut type_params = trait_.type_params.clone();
    let mut substitutions = HashMap::new();
    let bindings = trait_
        .associated_types
        .iter()
        .map(|associated_type| {
            let name = format!("{}{}", trait_.name, associated_type.name);
            type_params.push(name.clone());
            let type_ref = TypeRef {
                name: name.clone(),
                args: Vec::new(),
                bindings: Vec::new(),
                function: None,
                span: associated_type.span,
            };
            substitutions.insert(format!("Self.{}", associated_type.name), type_ref.clone());
            crate::ast::AssociatedTypeBinding {
                name: associated_type.name.clone(),
                type_ref,
                span: associated_type.span,
            }
        })
        .collect();
    let function = FunctionDecl {
        name: Some(method.name.clone()),
        exported: false,
        type_params: method.type_params.clone(),
        type_param_bounds: method
            .type_param_bounds
            .iter()
            .map(|bound| TypeParamBound {
                param: bound.param.clone(),
                trait_ref: substitute_type(&bound.trait_ref, &substitutions),
                span: bound.span,
            })
            .collect(),
        params: method
            .params
            .iter()
            .cloned()
            .map(|mut param| {
                param.type_ref = param
                    .type_ref
                    .as_ref()
                    .map(|type_ref| substitute_type(type_ref, &substitutions));
                param
            })
            .collect(),
        return_type: method
            .return_type
            .as_ref()
            .map(|type_ref| substitute_type(type_ref, &substitutions)),
        body,
        span: method.span,
    };

    Some(crate::ast::ExtensionDecl {
        type_ref: TypeRef {
            name: trait_.name.clone(),
            args: trait_
                .type_params
                .iter()
                .map(|name| TypeRef {
                    name: name.clone(),
                    args: Vec::new(),
                    bindings: Vec::new(),
                    function: None,
                    span: trait_.span,
                })
                .collect(),
            bindings,
            function: None,
            span: trait_.span,
        },
        exported: false,
        type_params,
        type_param_bounds: trait_.type_param_bounds.clone(),
        function,
        static_: false,
        span: method.span,
    })
}
