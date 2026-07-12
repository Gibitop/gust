impl Monomorphizer {
    fn infer_generic_method_returns(&mut self) {
        let mut templates = Vec::new();
        for template in self.struct_templates.values() {
            for member in &template.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function)
                        if !function.type_params.is_empty() =>
                    {
                        templates.push((
                            template.name.clone(),
                            function.clone(),
                            matches!(member, StructMember::StaticMethod(_)),
                        ));
                    }
                    StructMember::Field(_)
                    | StructMember::Method(_)
                    | StructMember::StaticMethod(_) => {}
                }
            }
        }
        for template in self.concrete_struct_defs.values() {
            for member in &template.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function)
                        if !function.type_params.is_empty() =>
                    {
                        templates.push((
                            template.name.clone(),
                            function.clone(),
                            matches!(member, StructMember::StaticMethod(_)),
                        ));
                    }
                    StructMember::Field(_)
                    | StructMember::Method(_)
                    | StructMember::StaticMethod(_) => {}
                }
            }
        }
        for template in self.enum_templates.values() {
            for member in &template.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function)
                        if !function.type_params.is_empty() =>
                    {
                        templates.push((
                            template.name.clone(),
                            function.clone(),
                            matches!(member, StructMember::StaticMethod(_)),
                        ));
                    }
                    StructMember::Field(_)
                    | StructMember::Method(_)
                    | StructMember::StaticMethod(_) => {}
                }
            }
        }
        for template in self.concrete_enums.values() {
            for member in &template.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function)
                        if !function.type_params.is_empty() =>
                    {
                        templates.push((
                            template.name.clone(),
                            function.clone(),
                            matches!(member, StructMember::StaticMethod(_)),
                        ));
                    }
                    StructMember::Field(_)
                    | StructMember::Method(_)
                    | StructMember::StaticMethod(_) => {}
                }
            }
        }

        for (owner, function, static_) in &templates {
            if let (Some(name), Some(return_type)) = (&function.name, &function.return_type) {
                self.generic_method_returns
                    .insert((owner.clone(), name.clone(), *static_), return_type.clone());
            }
        }

        for _ in 0..templates.len() {
            let mut changed = false;
            for (owner, function, static_) in &templates {
                let Some(name) = &function.name else {
                    continue;
                };
                let key = (owner.clone(), name.clone(), *static_);
                if self.generic_method_returns.contains_key(&key) {
                    continue;
                }
                let Some(return_type) =
                    self.infer_rewritten_function_return(function, owner, !static_)
                else {
                    continue;
                };
                self.generic_method_returns.insert(key, return_type);
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    fn infer_generic_function_returns(&mut self) {
        for _ in 0..self.function_templates.len() {
            let mut changed = false;
            for (name, template) in self.function_templates.clone() {
                if self.generic_function_returns.contains_key(&name) {
                    continue;
                }
                let Some(return_type) = self.infer_rewritten_function_return(&template, "", false)
                else {
                    continue;
                };
                self.generic_function_returns.insert(name, return_type);
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

}
