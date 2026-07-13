fn find_method_member(
    members: &[StructMember],
    method_name: &str,
    static_: bool,
) -> Option<FunctionDecl> {
    members.iter().find_map(|member| match member {
        StructMember::Method(function) if !static_ => {
            (function.name.as_deref() == Some(method_name)).then(|| function.clone())
        }
        StructMember::StaticMethod(function) if static_ => {
            (function.name.as_deref() == Some(method_name)).then(|| function.clone())
        }
        StructMember::Field(_) | StructMember::Method(_) | StructMember::StaticMethod(_) => None,
    })
}

impl Monomorphizer {
    fn extension_receiver_type_params(&self, extension: &crate::ast::ExtensionDecl) -> Vec<String> {
        let mut params = Vec::new();
        for name in extension
            .type_params
            .iter()
            .map(String::as_str)
            .chain(type_arg_names(&extension.type_ref))
        {
            if !self.is_known_type_name(name) && !params.iter().any(|param| param == name) {
                params.push(name.to_string());
            }
        }
        params
    }

    fn is_generic_extension_template(&self, extension: &crate::ast::ExtensionDecl) -> bool {
        !extension.function.type_params.is_empty()
            || !self
                .extension_receiver_type_params(extension)
                .is_empty()
    }

    fn is_known_type_name(&self, name: &str) -> bool {
        crate::ast::BasicType::from_name(name).is_some()
            || name == "void"
            || self.struct_templates.contains_key(name)
            || self.enum_templates.contains_key(name)
            || self.trait_templates.contains_key(name)
            || self.concrete_structs.contains(name)
            || self.concrete_enums.contains_key(name)
            || self.concrete_traits.contains(name)
    }
}
fn concrete_type_name(type_ref: &TypeRef) -> Option<String> {
    type_ref.args.is_empty().then(|| type_ref.name.clone())
}

fn concrete_type_refs(items: &[Item]) -> Vec<TypeRef> {
    items
        .iter()
        .filter_map(|item| {
            let (name, span) = match item {
                Item::Struct(item) => (&item.name, item.span),
                Item::Enum(item) => (&item.name, item.span),
                _ => return None,
            };
            Some(TypeRef {
                name: name.clone(),
                args: Vec::new(),
                bindings: Vec::new(),
                function: None,
                span,
            })
        })
        .collect()
}

fn concrete_trait_refs(items: &[Item]) -> Vec<TypeRef> {
    items
        .iter()
        .filter_map(|item| {
            let Item::Trait(item) = item else {
                return None;
            };
            Some(TypeRef {
                name: item.name.clone(),
                args: Vec::new(),
                bindings: Vec::new(),
                function: None,
                span: item.span,
            })
        })
        .collect()
}

fn concrete_impl_exists(items: &[Item], trait_ref: &TypeRef, type_ref: &TypeRef) -> bool {
    let trait_name = type_name(trait_ref);
    let self_type_name = type_name(type_ref);
    items.iter().any(|item| {
        let Item::Impl(item) = item else {
            return false;
        };
        let candidate_trait_name = type_name(&item.trait_ref);
        let trait_matches = candidate_trait_name == trait_name
            || (!trait_name.contains("<type ")
                && !trait_name.contains(", type ")
                && impl_trait_identity_name(item) == trait_name);
        trait_matches && type_name(&item.type_ref) == self_type_name
    })
}

fn impl_trait_identity_name(item: &ImplDecl) -> String {
    let name = type_name(&item.trait_ref);
    let marker = item
        .associated_types
        .iter()
        .filter_map(|associated_type| {
            let first = format!("<type {}: ", associated_type.name);
            let later = format!(", type {}: ", associated_type.name);
            name.find(&first)
                .map(|index| (index, false))
                .or_else(|| name.find(&later).map(|index| (index, true)))
        })
        .min_by_key(|(index, _)| *index);
    match marker {
        Some((index, true)) => format!("{}>", &name[..index]),
        Some((index, false)) => name[..index].to_string(),
        None => name,
    }
}

fn type_names(type_ref: &TypeRef) -> Vec<&str> {
    if let Some(function) = &type_ref.function {
        let mut names = Vec::new();
        for param in &function.params {
            names.extend(type_names(&param.type_ref));
        }
        names.extend(type_names(&function.return_type));
        return names;
    }
    let mut names = vec![type_ref.name.as_str()];
    for arg in &type_ref.args {
        names.extend(type_names(arg));
    }
    for binding in &type_ref.bindings {
        names.extend(type_names(&binding.type_ref));
    }
    names
}

fn type_arg_names(type_ref: &TypeRef) -> Vec<&str> {
    if let Some(function) = &type_ref.function {
        let mut names = Vec::new();
        for param in &function.params {
            names.extend(type_names(&param.type_ref));
        }
        names.extend(type_names(&function.return_type));
        return names;
    }
    type_ref.args.iter().flat_map(type_names).collect()
}

fn consistent_type(types: &[TypeRef]) -> Option<TypeRef> {
    let first = types.first()?;
    types
        .iter()
        .all(|type_ref| type_name(type_ref) == type_name(first))
        .then(|| first.clone())
}

fn substitute_type(type_ref: &TypeRef, substitutions: &HashMap<String, TypeRef>) -> TypeRef {
    if type_ref.args.is_empty()
        && let Some(substitution) = substitutions.get(&type_ref.name)
    {
        let mut substitution = substitution.clone();
        substitution.span = type_ref.span;
        return substitution;
    }

    TypeRef {
        name: type_ref.name.clone(),
        args: type_ref
            .args
            .iter()
            .map(|arg| substitute_type(arg, substitutions))
            .collect(),
        bindings: type_ref
            .bindings
            .iter()
            .map(|binding| crate::ast::AssociatedTypeBinding {
                name: binding.name.clone(),
                type_ref: substitute_type(&binding.type_ref, substitutions),
                span: binding.span,
            })
            .collect(),
        function: type_ref
            .function
            .as_ref()
            .map(|function| crate::ast::FunctionTypeRef {
                params: function
                    .params
                    .iter()
                    .map(|param| crate::ast::FunctionTypeParam {
                        mutable: param.mutable,
                        type_ref: substitute_type(&param.type_ref, substitutions),
                    })
                    .collect(),
                return_type: Box::new(substitute_type(&function.return_type, substitutions)),
            }),
        span: type_ref.span,
    }
}

fn specialized_name(name: &str, args: &[TypeRef]) -> String {
    let args = args.iter().map(type_name).collect::<Vec<_>>().join(", ");
    format!("{name}<{args}>")
}

fn specialized_trait_name(
    name: &str,
    args: &[TypeRef],
    bindings: &[crate::ast::AssociatedTypeBinding],
) -> String {
    if bindings.is_empty() {
        return specialized_name(name, args);
    }
    let mut parts = args.iter().map(type_name).collect::<Vec<_>>();
    parts.extend(
        bindings
            .iter()
            .map(|binding| {
                format!(
                    "type {}: {}",
                    binding.name,
                    type_name(&binding.type_ref)
                )
            }),
    );
    format!("{name}<{}>", parts.join(", "))
}

fn type_name(type_ref: &TypeRef) -> String {
    if let Some(function) = &type_ref.function {
        let params = function
            .params
            .iter()
            .map(|param| {
                let type_name = type_name(&param.type_ref);
                if param.mutable {
                    format!("mut {type_name}")
                } else {
                    type_name
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        return format!("fn({params}): {}", type_name(&function.return_type));
    }

    if type_ref.args.is_empty() && type_ref.bindings.is_empty() {
        type_ref.name.clone()
    } else {
        specialized_trait_name(&type_ref.name, &type_ref.args, &type_ref.bindings)
    }
}

fn requested_trait_method(name: &str) -> (Option<&str>, &str) {
    name.rsplit_once("::")
        .map_or((None, name), |(trait_name, method_name)| {
            (Some(trait_name), method_name)
        })
}

fn trait_name_matches_request(actual: &str, requested: &str) -> bool {
    actual == requested || actual.rsplit("::").next() == Some(requested)
}
