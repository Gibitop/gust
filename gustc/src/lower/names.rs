fn method_name(struct_name: &str, method_name: &str) -> String {
    format!("{struct_name}.{method_name}")
}

fn extension_name(type_name: &str, function_name: &str) -> String {
    format!("extension {type_name}.{function_name}")
}

fn is_raw_buffer_name(name: &str) -> bool {
    name == "RawBuffer"
        || name.ends_with("::RawBuffer")
        || name.contains("::RawBuffer<")
        || name.starts_with("RawBuffer<")
}

fn raw_buffer_element_name(name: &str) -> Option<&str> {
    let start = name.rfind("RawBuffer<")? + "RawBuffer<".len();
    let element = name.get(start..name.len().checked_sub(1)?)?;
    (!element.is_empty() && name.ends_with('>')).then_some(element)
}

fn lowered_type_from_concrete_name(
    name: &str,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    if let Some(type_) = BasicType::from_name(name) {
        return Some(LoweredType::Basic(type_));
    }

    if name == "void" {
        return Some(LoweredType::Void);
    }

    if structs.contains_key(name) {
        return Some(LoweredType::Struct(name.to_string()));
    }

    if enums.contains_key(name) {
        return Some(LoweredType::Enum(name.to_string()));
    }

    if traits.contains_key(name) {
        return Some(LoweredType::Trait(name.to_string()));
    }

    None
}

fn is_string_builder_name(name: &str) -> bool {
    name == "StringBuilder" || name.ends_with("::StringBuilder")
}

fn trait_method_name(type_name: &str, function_name: &str) -> String {
    format!("trait {type_name}.{function_name}")
}

fn qualified_trait_method_name(trait_name: &str, type_name: &str, function_name: &str) -> String {
    format!("trait {trait_name} for {type_name}.{function_name}")
}

fn qualified_static_trait_method_name(
    trait_name: &str,
    type_name: &str,
    function_name: &str,
) -> String {
    format!("static trait {trait_name} for {type_name}.{function_name}")
}

fn static_trait_method_name(type_name: &str, function_name: &str) -> String {
    format!("static trait {type_name}.{function_name}")
}

fn source_callable_name(name: &str) -> &str {
    name.rsplit_once("::").map_or(name, |(_, name)| name)
}

fn requested_trait_name(name: &str) -> Option<&str> {
    name.rsplit_once("::").map(|(trait_name, _)| trait_name)
}

fn static_method_name(type_name: &str, function_name: &str) -> String {
    format!("static {type_name}.{function_name}")
}

fn static_extension_name(type_name: &str, function_name: &str) -> String {
    format!("static extension {type_name}.{function_name}")
}

fn callable_method_name(
    type_: &LoweredType,
    name: &str,
    signatures: &HashMap<String, FunctionSignature>,
) -> Option<String> {
    match type_ {
        LoweredType::Struct(type_name) | LoweredType::Enum(type_name) => {
            let name = method_name(type_name, source_callable_name(name));
            if signatures.contains_key(&name) {
                return Some(name);
            }
        }
        _ => {}
    }

    let extension = extension_name(&type_.name(), name);
    if signatures.contains_key(&extension) {
        return Some(extension);
    }

    if let Some(trait_name) = requested_trait_name(name) {
        let qualified_name =
            qualified_trait_method_name(trait_name, &type_.name(), source_callable_name(name));
        if signatures.contains_key(&qualified_name) {
            return Some(qualified_name);
        }
    }

    let name = trait_method_name(&type_.name(), source_callable_name(name));
    signatures.contains_key(&name).then_some(name)
}

fn callable_static_name(
    type_: &LoweredType,
    name: &str,
    signatures: &HashMap<String, FunctionSignature>,
) -> Option<String> {
    let method_name = static_method_name(&type_.name(), source_callable_name(name));
    if signatures.contains_key(&method_name) {
        return Some(method_name);
    }

    let extension = static_extension_name(&type_.name(), name);
    if signatures.contains_key(&extension) {
        return Some(extension);
    }

    if let Some(trait_name) = requested_trait_name(name) {
        let qualified_name = qualified_static_trait_method_name(
            trait_name,
            &type_.name(),
            source_callable_name(name),
        );
        if signatures.contains_key(&qualified_name) {
            return Some(qualified_name);
        }
    }

    let name = static_trait_method_name(&type_.name(), source_callable_name(name));
    signatures.contains_key(&name).then_some(name)
}

fn trait_impl_method_name(
    trait_: &LoweredTrait,
    self_type: &LoweredType,
    method_name: &str,
) -> Option<String> {
    trait_
        .impls
        .iter()
        .find(|impl_| impl_.self_type == *self_type)?
        .methods
        .iter()
        .find(|method| method.name == method_name)
        .map(|method| method.function_name.clone())
}

fn for_iterator_local_name(span: Span) -> String {
    format!("internal_for_iterator_{}_{}", span.start, span.end)
}

fn find_lowered_struct_by_source_name(
    source_name: &str,
    structs: &HashMap<String, LoweredStruct>,
) -> Option<String> {
    structs
        .keys()
        .find_map(|name| (source_callable_name(name) == source_name).then(|| name.clone()))
}

fn match_temp_name(span: Span) -> String {
    format!("internal_match_value_{}", span.start)
}

