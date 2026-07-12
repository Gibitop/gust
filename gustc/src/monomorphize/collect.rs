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
        let trait_templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Trait(item) = item else {
                    return None;
                };
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
        let extensions = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Extension(item) = item else {
                    return None;
                };
                Some(item.clone())
            })
            .collect();
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
            trait_templates,
            impl_declarations,
            impl_templates,
            extensions,
            function_templates,
            concrete_structs,
            concrete_struct_defs,
            concrete_enums,
            concrete_traits,
            pending: VecDeque::new(),
            emitted: HashSet::new(),
            specializations: HashMap::new(),
            scopes: Vec::new(),
            return_types: Vec::new(),
            self_types: Vec::new(),
            inferred_returns: Vec::new(),
            member_returns: HashMap::new(),
            function_returns,
            function_params,
            generic_function_returns,
            generic_method_returns: HashMap::new(),
            expected_expr_types: HashMap::new(),
            inferred_expr_types: HashMap::new(),
            impl_receiver_types: Vec::new(),
            bound_checks: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

}
