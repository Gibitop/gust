fn push_c_local_name(source: &mut String, name: &str) {
    source.push_str("gust_");
    source.push_str(name);
}

fn closure_env_type_name(name: &str) -> String {
    format!(
        "gust_env_{:08x}_{}",
        stable_name_hash(name),
        sanitized_name(name)
    )
}

fn sanitized_name(name: &str) -> String {
    let mut result = String::new();
    push_c_identifier_suffix(&mut result, name);
    result
}

fn type_name_key(type_: &LoweredType) -> String {
    match type_ {
        LoweredType::Basic(type_) => type_.name().to_string(),
        LoweredType::Struct(name) | LoweredType::Enum(name) | LoweredType::Trait(name) => {
            name.clone()
        }
        LoweredType::Function {
            params,
            return_type,
        } => {
            let params = params
                .iter()
                .map(|param| {
                    if param.mutable {
                        format!("mut {}", type_name_key(&param.type_))
                    } else {
                        type_name_key(&param.type_)
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("fn({params}):{}", type_name_key(return_type))
        }
        LoweredType::Void => "void".to_string(),
    }
}

fn stable_name_hash(name: &str) -> u32 {
    let mut hash = 0x811c9dc5_u32;

    for byte in name.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x01000193);
    }

    hash
}

fn push_c_identifier_suffix(source: &mut String, name: &str) {
    for byte in name.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => source.push(byte as char),
            _ => source.push('_'),
        }
    }
}

