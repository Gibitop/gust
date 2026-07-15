struct ProjectLoader {
    modules: Vec<Module>,
    module_indexes: HashMap<(usize, PathBuf), usize>,
    loading: HashSet<(usize, PathBuf)>,
    packages: Vec<Package>,
    sources: Vec<SourceFile>,
    diagnostics: Vec<Diagnostic>,
    next_offset: usize,
    load_error: Option<String>,
}

impl ProjectLoader {
    fn new(packages: Vec<Package>) -> Self {
        Self {
            modules: Vec::new(),
            module_indexes: HashMap::new(),
            loading: HashSet::new(),
            packages,
            sources: Vec::new(),
            diagnostics: Vec::new(),
            next_offset: 0,
            load_error: None,
        }
    }

    fn load_module(
        &mut self,
        path: PathBuf,
        key: String,
        package: usize,
        entry: bool,
        import_span: Option<Span>,
    ) -> Option<usize> {
        let module_id = (package, path.clone());
        if self.loading.contains(&module_id) {
            if let Some(span) = import_span {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("module import cycle reaches `{}`", path.display()),
                ));
            }
            return self.module_indexes.get(&module_id).copied();
        }

        if let Some(index) = self.module_indexes.get(&module_id) {
            return Some(*index);
        }

        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                if let Some(span) = import_span {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        format!("failed to read module `{}`: {error}", path.display()),
                    ));
                    return None;
                }

                self.load_error = Some(format!(
                    "failed to read entry module `{}`: {error}",
                    path.display()
                ));
                return None;
            }
        };
        let offset = self.next_offset;
        self.next_offset += source.len() + 1;
        self.sources.push(SourceFile {
            path: path.clone(),
            source: source.clone(),
            offset,
        });

        let (tokens, lexer_diagnostics) = Lexer::new(&source).tokenize();
        let (mut program, parser_diagnostics) = Parser::new(tokens).parse();
        self.diagnostics.extend(
            lexer_diagnostics
                .into_iter()
                .chain(parser_diagnostics)
                .map(|diagnostic| shift_diagnostic(diagnostic, offset)),
        );
        shift_program(&mut program, offset);

        let index = self.modules.len();
        self.module_indexes.insert(module_id.clone(), index);
        self.loading.insert(module_id.clone());
        let imports = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Import(import) => Some(ResolvedImport {
                    path: import.path.clone(),
                    names: import.names.clone(),
                    namespace: import.namespace.clone(),
                    span: import.span,
                    target: None,
                }),
                _ => None,
            })
            .collect();
        self.modules.push(Module {
            path: path.clone(),
            key: key.clone(),
            package,
            program,
            imports,
            entry,
        });

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let import_count = self.modules[index].imports.len();
        for import_index in 0..import_count {
            let import_path = self.modules[index].imports[import_index].path.clone();
            let span = self.modules[index].imports[import_index].span;
            let Some((resolved_package, resolved_path)) =
                self.resolve_import(package, parent, &import_path, span)
            else {
                continue;
            };
            let resolved_key = self.module_key(resolved_package, &resolved_path);
            let target =
                self.load_module(resolved_path, resolved_key, resolved_package, false, Some(span));
            self.modules[index].imports[import_index].target = target;
        }

        self.loading.remove(&module_id);
        Some(index)
    }

    fn resolve_import(
        &mut self,
        package: usize,
        parent: &Path,
        import_path: &str,
        span: Span,
    ) -> Option<(usize, PathBuf)> {
        if import_path.starts_with('.') {
            let mut path = parent.join(import_path);
            if path.extension().is_none() {
                path.set_extension("gust");
            }
            let path = self.canonicalize_import_path(path, span)?;
            if self.packages[package].project && !path.starts_with(&self.packages[package].src) {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!(
                        "relative import `{import_path}` escapes project source directory `{}`",
                        self.packages[package].src.display()
                    ),
                ));
                return None;
            }
            return Some((package, path));
        }

        let (dependency_name, module_path) = import_path
            .split_once('/')
            .map_or((import_path, ""), |(dependency_name, module_path)| {
                (dependency_name, module_path)
            });
        let Some(&dependency_package) = self.packages[package].dependencies.get(dependency_name)
        else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("unknown package dependency `{dependency_name}`"),
            ));
            return None;
        };

        let mut path = self.packages[dependency_package].src.clone();
        if module_path.is_empty() {
            path.push("lib");
        } else {
            path.push(module_path);
        }
        if path.extension().is_none() {
            path.set_extension("gust");
        }
        let path = self.canonicalize_import_path(path, span)?;
        if !path.starts_with(&self.packages[dependency_package].src) {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "package import `{import_path}` resolves outside dependency source directory `{}`",
                    self.packages[dependency_package].src.display()
                ),
            ));
            return None;
        }

        Some((dependency_package, path))
    }

    fn canonicalize_import_path(&mut self, path: PathBuf, span: Span) -> Option<PathBuf> {
        match path.canonicalize() {
            Ok(path) => Some(path),
            Err(error) => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("failed to resolve module `{}`: {error}", path.display()),
                ));
                None
            }
        }
    }

    fn module_key(&self, package: usize, path: &Path) -> String {
        let package = &self.packages[package];
        let module_path = path.strip_prefix(&package.src).unwrap_or(path);
        format!(
            "{}:{}",
            package.root.to_string_lossy(),
            module_path.to_string_lossy()
        )
    }
}
