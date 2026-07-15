fn link_modules(modules: &[Module], diagnostics: &mut Vec<Diagnostic>) -> Program {
    let mut exports = Vec::with_capacity(modules.len());
    let mut local_names = Vec::with_capacity(modules.len());
    let mut local_extensions = Vec::with_capacity(modules.len());

    for module in modules {
        let mut module_exports = HashMap::new();
        let mut module_names = HashMap::new();
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
            }
        }

        exports.push(module_exports);
        local_names.push(module_names);
        local_extensions.push(module_extensions);
    }

    let mut visible_names = local_names.clone();
    let mut visible_extensions = local_extensions.clone();
    let mut visible_namespaces = vec![HashMap::new(); modules.len()];
    for (module_index, module) in modules.iter().enumerate() {
        for import in &module.imports {
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
                if export.extension {
                    if visible_extensions[module_index]
                        .insert(local_name.clone(), export.internal_name.clone())
                        .is_some()
                    {
                        diagnostics.push(Diagnostic::error(
                            imported_name.span,
                            format!(
                                "imported extension `{local_name}` conflicts with another extension in this module"
                            ),
                        ));
                    }
                    continue;
                }
                if visible_names[module_index]
                    .insert(local_name.clone(), export.internal_name.clone())
                    .is_some()
                    || visible_namespaces[module_index].contains_key(local_name)
                {
                    diagnostics.push(Diagnostic::error(
                        imported_name.span,
                        format!(
                            "imported name `{local_name}` conflicts with another name in this module"
                        ),
                    ));
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
