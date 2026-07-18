fn link_modules(modules: &[Module], diagnostics: &mut Vec<Diagnostic>) -> Program {
    let mut exports = Vec::with_capacity(modules.len());
    let mut local_names = Vec::with_capacity(modules.len());
    let mut local_name_packages = Vec::with_capacity(modules.len());
    let mut local_extensions = Vec::with_capacity(modules.len());

    for module in modules {
        let mut module_exports = HashMap::new();
        let mut module_names = HashMap::new();
        let mut module_name_packages = HashMap::new();
        let mut module_extensions = HashMap::new();
        let mut declared_names = HashMap::new();

        for item in &module.program.items {
            let Some(declaration) = item_declaration(item) else {
                continue;
            };
            let internal_name = if declaration.extension {
                qualified_extension_name(&module.key, declaration.name)
            } else if module.entry {
                declaration.name.to_string()
            } else {
                qualified_name(&module.key, declaration.name)
            };
            if declaration.exported {
                let export = Export {
                    internal_name: internal_name.clone(),
                    package: module.package,
                    extension: declaration.extension,
                };
                module_exports.insert(declaration.name.to_string(), export);
            }
            if let Some(previous_extension) =
                declared_names.insert(declaration.name.to_string(), declaration.extension)
                && !(previous_extension && declaration.extension)
            {
                diagnostics.push(Diagnostic::error(
                    declaration.span,
                    format!(
                        "duplicate top-level name `{}` in this module",
                        declaration.name
                    ),
                ));
            }
            if declaration.extension {
                if declaration.name == "clone" {
                    diagnostics.push(Diagnostic::error(
                        declaration.span,
                        "extension function name `clone` is reserved for the built-in deep clone operation",
                    ));
                }
                module_extensions.insert(declaration.name.to_string(), internal_name);
            } else {
                module_names.insert(declaration.name.to_string(), internal_name);
                module_name_packages.insert(declaration.name.to_string(), module.package);
            }
        }

        exports.push(module_exports);
        local_names.push(module_names);
        local_name_packages.push(module_name_packages);
        local_extensions.push(module_extensions);
    }

    resolve_re_exports(modules, &mut exports, diagnostics);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Program { items: Vec::new() };
    }

    let mut visible_names = local_names.clone();
    let mut visible_name_packages = local_name_packages.clone();
    let mut visible_extensions = local_extensions.clone();
    let mut visible_namespaces = vec![HashMap::new(); modules.len()];
    for weak in [false, true] {
        for (module_index, module) in modules.iter().enumerate() {
            for import in &module.imports {
                if import.exported || import.weak != weak {
                    continue;
                }
                let Some(target) = import.target else {
                    continue;
                };

                if let Some(namespace) = &import.namespace
                    && (visible_names[module_index].contains_key(&namespace.name)
                        || visible_namespaces[module_index]
                            .insert(namespace.name.clone(), target)
                            .is_some())
                {
                    diagnostics.push(Diagnostic::error(
                        namespace.span,
                        format!(
                            "module namespace `{}` conflicts with another name in this module",
                            namespace.name
                        ),
                    ));
                }

                if import.glob {
                    for (name, export) in &exports[target] {
                        import_visible_export(
                            module_index,
                            name,
                            export,
                            import.span,
                            import.weak,
                            &mut visible_names,
                            &mut visible_name_packages,
                            &mut visible_extensions,
                            &visible_namespaces,
                            diagnostics,
                        );
                    }
                }

                for imported_name in &import.names {
                    let name = &imported_name.name;
                    let local_name = imported_name.alias.as_ref().unwrap_or(name);
                    let Some(export) = exports[target].get(name) else {
                        diagnostics.push(Diagnostic::error(
                            imported_name.span,
                            format!(
                                "module `{}` does not export `{name}`",
                                modules[target].path.display()
                            ),
                        ));
                        continue;
                    };
                    import_visible_export(
                        module_index,
                        local_name,
                        export,
                        imported_name.span,
                        import.weak,
                        &mut visible_names,
                        &mut visible_name_packages,
                        &mut visible_extensions,
                        &visible_namespaces,
                        diagnostics,
                    );
                }
            }
        }
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Program { items: Vec::new() };
    }

    validate_foreign_impls(modules, &visible_name_packages, diagnostics);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Program { items: Vec::new() };
    }

    let mut items = Vec::new();
    for (module_index, module) in modules.iter().enumerate() {
        let mut rewriter = ModuleRewriter::new(
            &local_names[module_index],
            &visible_names[module_index],
            &local_extensions[module_index],
            &visible_extensions[module_index],
            &visible_namespaces[module_index],
            &exports,
            diagnostics,
            module.entry,
        );

        for item in &module.program.items {
            if matches!(item, Item::Import(_)) {
                continue;
            }
            let mut item = item.clone();
            rewriter.rewrite_item(&mut item);
            items.push(item);
        }
    }

    Program { items }
}

fn resolve_re_exports(
    modules: &[Module],
    exports: &mut [HashMap<String, Export>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut states = vec![ReExportState::Pending; modules.len()];
    for module_index in 0..modules.len() {
        resolve_module_re_exports(module_index, modules, exports, diagnostics, &mut states);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReExportState {
    Pending,
    Visiting,
    Done,
}

fn resolve_module_re_exports(
    module_index: usize,
    modules: &[Module],
    exports: &mut [HashMap<String, Export>],
    diagnostics: &mut Vec<Diagnostic>,
    states: &mut [ReExportState],
) {
    match states[module_index] {
        ReExportState::Done => return,
        ReExportState::Visiting => return,
        ReExportState::Pending => {}
    }
    states[module_index] = ReExportState::Visiting;

    for import in &modules[module_index].imports {
        if !import.exported {
            continue;
        }
        let Some(target) = import.target else {
            continue;
        };
        resolve_module_re_exports(target, modules, exports, diagnostics, states);

        if import.glob {
            for (name, export) in exports[target].clone() {
                export_visible_name(
                    module_index,
                    &name,
                    export,
                    import.span,
                    exports,
                    diagnostics,
                );
            }
        }

        for imported_name in &import.names {
            let name = &imported_name.name;
            let export_name = imported_name.alias.as_ref().unwrap_or(name);
            let Some(export) = exports[target].get(name).cloned() else {
                diagnostics.push(Diagnostic::error(
                    imported_name.span,
                    format!(
                        "module `{}` does not export `{name}`",
                        modules[target].path.display()
                    ),
                ));
                continue;
            };
            export_visible_name(
                module_index,
                export_name,
                export,
                imported_name.span,
                exports,
                diagnostics,
            );
        }
    }

    states[module_index] = ReExportState::Done;
}

fn export_visible_name(
    module_index: usize,
    name: &str,
    export: Export,
    span: Span,
    exports: &mut [HashMap<String, Export>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(existing) = exports[module_index].get(name) {
        if existing.internal_name == export.internal_name {
            return;
        }
        diagnostics.push(Diagnostic::error(
            span,
            format!("re-exported name `{name}` conflicts with another export in this module"),
        ));
        return;
    }

    exports[module_index].insert(name.to_string(), export);
}

#[allow(clippy::too_many_arguments)]
fn import_visible_export(
    module_index: usize,
    local_name: &str,
    export: &Export,
    span: Span,
    weak: bool,
    visible_names: &mut [HashMap<String, String>],
    visible_name_packages: &mut [HashMap<String, usize>],
    visible_extensions: &mut [HashMap<String, String>],
    visible_namespaces: &[HashMap<String, usize>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    if export.extension {
        if visible_extensions[module_index].contains_key(local_name) {
            if !weak {
                diagnostics.push(Diagnostic::error(
                    span,
                    format!(
                        "imported extension `{local_name}` conflicts with another extension in this module"
                    ),
                ));
            }
            return;
        }
        visible_extensions[module_index].insert(
            local_name.to_string(),
            export.internal_name.clone(),
        );
        return;
    }

    if visible_names[module_index].contains_key(local_name)
        || visible_namespaces[module_index].contains_key(local_name)
    {
        if !weak {
            diagnostics.push(Diagnostic::error(
                span,
                format!("imported name `{local_name}` conflicts with another name in this module"),
            ));
        }
        return;
    }

    visible_names[module_index].insert(local_name.to_string(), export.internal_name.clone());
    visible_name_packages[module_index].insert(local_name.to_string(), export.package);
}

struct ModuleDeclaration<'item> {
    name: &'item str,
    exported: bool,
    extension: bool,
    span: Span,
}

fn item_declaration(item: &Item) -> Option<ModuleDeclaration<'_>> {
    match item {
        Item::Enum(item) => Some(ModuleDeclaration {
            name: &item.name,
            exported: item.exported,
            extension: false,
            span: item.span,
        }),
        Item::Struct(item) => Some(ModuleDeclaration {
            name: &item.name,
            exported: item.exported,
            extension: false,
            span: item.span,
        }),
        Item::Trait(item) => Some(ModuleDeclaration {
            name: &item.name,
            exported: item.exported,
            extension: false,
            span: item.span,
        }),
        Item::Function(item) => item.name.as_deref().map(|name| ModuleDeclaration {
            name,
            exported: item.exported,
            extension: false,
            span: item.span,
        }),
        Item::Extension(item) => item
            .function
            .name
            .as_deref()
            .map(|name| ModuleDeclaration {
                name,
                exported: item.exported,
                extension: true,
                span: item.span,
            }),
        Item::Import(_) | Item::Impl(_) => None,
    }
}

fn qualified_name(module_key: &str, name: &str) -> String {
    format!("module_{:08x}::{name}", stable_name_hash(module_key))
}

fn qualified_extension_name(module_key: &str, name: &str) -> String {
    format!(
        "module_extension_{:08x}::{name}",
        stable_name_hash(module_key)
    )
}

fn stable_name_hash(name: &str) -> u32 {
    let mut hash = 0x811c9dc5_u32;

    for byte in name.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x01000193);
    }

    hash
}

fn validate_foreign_impls(
    modules: &[Module],
    visible_name_packages: &[HashMap<String, usize>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (module_index, module) in modules.iter().enumerate() {
        for item in &module.program.items {
            let Item::Impl(item) = item else {
                continue;
            };
            let Some(trait_package) = visible_name_packages[module_index].get(&item.trait_ref.name)
            else {
                continue;
            };
            if *trait_package == module.package {
                continue;
            }
            let Some(type_local) = impl_self_type_is_local(
                &item.type_ref,
                module.package,
                &visible_name_packages[module_index],
                &item.type_params,
            ) else {
                continue;
            };
            if type_local {
                continue;
            }
            diagnostics.push(Diagnostic::error(
                item.span,
                format!(
                    "cannot implement foreign trait `{}` for foreign type `{}`",
                    item.trait_ref.name, item.type_ref.name
                ),
            ));
        }
    }
}

fn impl_self_type_is_local(
    type_ref: &TypeRef,
    current_package: usize,
    visible_name_packages: &HashMap<String, usize>,
    impl_type_params: &[String],
) -> Option<bool> {
    if type_ref.function.is_some()
        || BasicType::from_name(&type_ref.name).is_some()
        || type_ref.name == "void"
        || impl_type_params.iter().any(|param| param == &type_ref.name)
    {
        return Some(false);
    }

    visible_name_packages
        .get(&type_ref.name)
        .map(|package| *package == current_package)
}
