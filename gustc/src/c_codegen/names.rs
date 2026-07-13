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

fn push_c_struct_desc_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_desc_struct_");
    push_hashed_c_name(source, name);
}

fn push_c_struct_trace_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_trace_struct_");
    push_hashed_c_name(source, name);
}

fn push_c_enum_desc_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_desc_enum_");
    push_hashed_c_name(source, name);
}

fn push_c_enum_trace_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_trace_enum_");
    push_hashed_c_name(source, name);
}

fn push_c_enum_box_trace_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_trace_enum_box_");
    push_hashed_c_name(source, name);
}

fn push_c_cell_desc_name(source: &mut String, type_: &LoweredType) {
    source.push_str("gust_rt_desc_cell_");
    push_hashed_c_name(source, &type_name_key(type_));
}

fn push_c_cell_trace_name(source: &mut String, type_: &LoweredType) {
    source.push_str("gust_rt_trace_cell_");
    push_hashed_c_name(source, &type_name_key(type_));
}

fn push_c_closure_env_desc_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_desc_");
    source.push_str(&closure_env_type_name(name));
}

fn push_c_root_name(source: &mut String, name: &str) {
    source.push_str("gust_rt_root_");
    push_c_identifier_suffix(source, name);
}

fn push_hashed_c_name(source: &mut String, name: &str) {
    source.push_str(&format!("{:08x}_", stable_name_hash(name)));
    push_c_identifier_suffix(source, name);
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
