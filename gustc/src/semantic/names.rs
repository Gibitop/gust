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

fn trait_item_type_name<'a>(trait_name: &'a str, protocol: &str) -> Option<&'a str> {
    let (trait_head, arguments) = trait_name.split_once('<')?;
    if source_callable_name(trait_head) != protocol {
        return None;
    }
    let arguments = arguments.strip_suffix('>')?;
    arguments
        .split(", type ")
        .find_map(|argument| {
            argument
                .trim_start()
                .strip_prefix("type Item: ")
                .or_else(|| argument.trim_start().strip_prefix("Item: "))
        })
        .or_else(|| (!arguments.trim_start().starts_with("type ")).then_some(arguments))
}

fn requested_trait_name(name: &str) -> Option<&str> {
    name.rsplit_once("::").map(|(trait_name, _)| trait_name)
}

fn trait_has_positional_type_arguments(name: &str) -> bool {
    name.split_once('<')
        .and_then(|(_, arguments)| arguments.strip_suffix('>'))
        .and_then(|arguments| arguments.split(',').next())
        .is_some_and(|argument| !argument.trim_start().starts_with("type "))
}
