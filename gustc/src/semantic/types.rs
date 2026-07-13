#[derive(Debug, Clone, PartialEq, Eq)]
enum Type {
    Basic(BasicType),
    Struct(String),
    Enum(String),
    Trait(String),
    Function {
        params: Vec<FunctionTypeParam>,
        return_type: Box<Type>,
    },
    Void,
    Named(String),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionTypeParam {
    type_: Type,
    mutable: bool,
}

impl Type {
    fn name(&self) -> String {
        match self {
            Type::Basic(type_) => type_.name().to_string(),
            Type::Struct(name) => name.clone(),
            Type::Enum(name) => name.clone(),
            Type::Trait(name) => name.clone(),
            Type::Function {
                params,
                return_type,
            } => {
                let params = params
                    .iter()
                    .map(|param| {
                        if param.mutable {
                            format!("mut {}", param.type_.name())
                        } else {
                            param.type_.name()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fn({params}): {}", return_type.name())
            }
            Type::Void => "void".to_string(),
            Type::Named(name) => name.clone(),
            Type::Unknown => "unknown".to_string(),
        }
    }
}

fn signatures_match(expected: &FunctionSignature, actual: &FunctionSignature) -> bool {
    expected.mutable_self == actual.mutable_self
        && expected.params.len() == actual.params.len()
        && expected
            .params
            .iter()
            .zip(&actual.params)
            .all(|(expected, actual)| {
                expected.mutable == actual.mutable && expected.type_ == actual.type_
            })
        && expected.return_type == actual.return_type
}

fn signature_contains_associated_projection(signature: &FunctionSignature) -> bool {
    signature
        .params
        .iter()
        .any(|param| type_contains_associated_projection(&param.type_))
        || type_contains_associated_projection(&signature.return_type)
}

fn type_contains_associated_projection(type_: &Type) -> bool {
    match type_ {
        Type::Named(name) => name.starts_with("Self."),
        Type::Function {
            params,
            return_type,
        } => {
            params
                .iter()
                .any(|param| type_contains_associated_projection(&param.type_))
                || type_contains_associated_projection(return_type)
        }
        Type::Basic(_) | Type::Struct(_) | Type::Enum(_) | Type::Trait(_) | Type::Void | Type::Unknown => {
            false
        }
    }
}

fn signature_with_self_type(signature: &FunctionSignature, self_type: &Type) -> FunctionSignature {
    FunctionSignature {
        params: signature
            .params
            .iter()
            .map(|param| ParamSignature {
                type_: type_with_self_type(&param.type_, self_type),
                mutable: param.mutable,
            })
            .collect(),
        return_type: type_with_self_type(&signature.return_type, self_type),
        mutable_self: signature.mutable_self,
    }
}

fn type_with_self_type(type_: &Type, self_type: &Type) -> Type {
    match type_ {
        Type::Named(name) if name == "Self" => self_type.clone(),
        Type::Function {
            params,
            return_type,
        } => Type::Function {
            params: params
                .iter()
                .map(|param| FunctionTypeParam {
                    type_: type_with_self_type(&param.type_, self_type),
                    mutable: param.mutable,
                })
                .collect(),
            return_type: Box::new(type_with_self_type(return_type, self_type)),
        },
        _ => type_.clone(),
    }
}
