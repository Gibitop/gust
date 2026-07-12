impl Monomorphizer {
    fn validate_impl_coherence(&mut self, program: &Program) {
        let impls = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Impl(item) = item else {
                    return None;
                };
                Some(item)
            })
            .collect::<Vec<_>>();

        for (index, impl_) in impls.iter().enumerate() {
            for previous in &impls[..index] {
                if impl_headers_overlap(previous, impl_) {
                    self.diagnostics.push(Diagnostic::error(
                        impl_.span,
                        format!(
                            "conflicting implementations of trait `{}` for type `{}`",
                            type_name(&impl_.trait_ref),
                            type_name(&impl_.type_ref)
                        ),
                    ));
                }
            }
        }
    }

}

#[derive(Clone, PartialEq, Eq)]
enum ImplHeaderType {
    Variable(usize, String),
    Named(String, Vec<ImplHeaderType>),
    Function {
        params: Vec<(bool, ImplHeaderType)>,
        return_type: Box<ImplHeaderType>,
    },
}

fn impl_headers_overlap(left: &ImplDecl, right: &ImplDecl) -> bool {
    let left_params = left.type_params.iter().cloned().collect::<HashSet<_>>();
    let right_params = right.type_params.iter().cloned().collect::<HashSet<_>>();
    let mut substitutions = HashMap::new();

    unify_impl_header_types(
        impl_header_type(&left.trait_ref, 0, &left_params),
        impl_header_type(&right.trait_ref, 1, &right_params),
        &mut substitutions,
    ) && unify_impl_header_types(
        impl_header_type(&left.type_ref, 0, &left_params),
        impl_header_type(&right.type_ref, 1, &right_params),
        &mut substitutions,
    )
}

fn impl_header_type(
    type_ref: &TypeRef,
    impl_index: usize,
    type_params: &HashSet<String>,
) -> ImplHeaderType {
    if type_ref.function.is_none()
        && type_ref.args.is_empty()
        && type_params.contains(&type_ref.name)
    {
        return ImplHeaderType::Variable(impl_index, type_ref.name.clone());
    }

    if let Some(function) = &type_ref.function {
        return ImplHeaderType::Function {
            params: function
                .params
                .iter()
                .map(|param| {
                    (
                        param.mutable,
                        impl_header_type(&param.type_ref, impl_index, type_params),
                    )
                })
                .collect(),
            return_type: Box::new(impl_header_type(
                &function.return_type,
                impl_index,
                type_params,
            )),
        };
    }

    ImplHeaderType::Named(
        type_ref.name.clone(),
        type_ref
            .args
            .iter()
            .map(|arg| impl_header_type(arg, impl_index, type_params))
            .collect(),
    )
}

fn unify_impl_header_types(
    left: ImplHeaderType,
    right: ImplHeaderType,
    substitutions: &mut HashMap<(usize, String), ImplHeaderType>,
) -> bool {
    let left = resolve_impl_header_type(left, substitutions);
    let right = resolve_impl_header_type(right, substitutions);

    if left == right {
        return true;
    }

    match (left, right) {
        (ImplHeaderType::Variable(index, name), type_)
        | (type_, ImplHeaderType::Variable(index, name)) => {
            bind_impl_header_variable((index, name), type_, substitutions)
        }
        (
            ImplHeaderType::Named(left_name, left_args),
            ImplHeaderType::Named(right_name, right_args),
        ) => {
            left_name == right_name
                && left_args.len() == right_args.len()
                && left_args
                    .into_iter()
                    .zip(right_args)
                    .all(|(left, right)| unify_impl_header_types(left, right, substitutions))
        }
        (
            ImplHeaderType::Function {
                params: left_params,
                return_type: left_return,
            },
            ImplHeaderType::Function {
                params: right_params,
                return_type: right_return,
            },
        ) => {
            left_params.len() == right_params.len()
                && left_params.into_iter().zip(right_params).all(
                    |((left_mutable, left), (right_mutable, right))| {
                        left_mutable == right_mutable
                            && unify_impl_header_types(left, right, substitutions)
                    },
                )
                && unify_impl_header_types(*left_return, *right_return, substitutions)
        }
        _ => false,
    }
}

fn resolve_impl_header_type(
    type_: ImplHeaderType,
    substitutions: &HashMap<(usize, String), ImplHeaderType>,
) -> ImplHeaderType {
    let ImplHeaderType::Variable(index, name) = &type_ else {
        return type_;
    };
    let Some(substitution) = substitutions.get(&(*index, name.clone())) else {
        return type_;
    };
    resolve_impl_header_type(substitution.clone(), substitutions)
}

fn bind_impl_header_variable(
    variable: (usize, String),
    type_: ImplHeaderType,
    substitutions: &mut HashMap<(usize, String), ImplHeaderType>,
) -> bool {
    if impl_header_type_contains_variable(&type_, &variable, substitutions) {
        return false;
    }
    substitutions.insert(variable, type_);
    true
}

fn impl_header_type_contains_variable(
    type_: &ImplHeaderType,
    variable: &(usize, String),
    substitutions: &HashMap<(usize, String), ImplHeaderType>,
) -> bool {
    match type_ {
        ImplHeaderType::Variable(index, name) => {
            let key = (*index, name.clone());
            if &key == variable {
                true
            } else {
                substitutions.get(&key).is_some_and(|substitution| {
                    impl_header_type_contains_variable(substitution, variable, substitutions)
                })
            }
        }
        ImplHeaderType::Named(_, args) => args
            .iter()
            .any(|arg| impl_header_type_contains_variable(arg, variable, substitutions)),
        ImplHeaderType::Function {
            params,
            return_type,
        } => {
            params.iter().any(|(_, param)| {
                impl_header_type_contains_variable(param, variable, substitutions)
            }) || impl_header_type_contains_variable(return_type, variable, substitutions)
        }
    }
}

