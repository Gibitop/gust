fn extension_name(type_name: &str, function_name: &str) -> String {
    format!("extension {type_name}.{function_name}")
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

fn generic_trait_item_type_name<'a>(trait_name: &'a str, protocol: &str) -> Option<&'a str> {
    let (trait_head, item_type) = trait_name.split_once('<')?;
    (source_callable_name(trait_head) == protocol)
        .then(|| item_type.strip_suffix('>'))
        .flatten()
}

fn requested_trait_name(name: &str) -> Option<&str> {
    name.rsplit_once("::").map(|(trait_name, _)| trait_name)
}

