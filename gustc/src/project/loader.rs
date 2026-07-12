struct ProjectLoader {
    modules: Vec<Module>,
    module_indexes: HashMap<PathBuf, usize>,
    loading: HashSet<PathBuf>,
    sources: Vec<SourceFile>,
    diagnostics: Vec<Diagnostic>,
    next_offset: usize,
    load_error: Option<String>,
}

impl ProjectLoader {
    fn new() -> Self {
        Self {
            modules: Vec::new(),
            module_indexes: HashMap::new(),
            loading: HashSet::new(),
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
        entry: bool,
        import_span: Option<Span>,
    ) -> Option<usize> {
        if self.loading.contains(&path) {
            if let Some(span) = import_span {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("module import cycle reaches `{}`", path.display()),
                ));
            }
            return self.module_indexes.get(&path).copied();
        }

        if let Some(index) = self.module_indexes.get(&path) {
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
        self.module_indexes.insert(path.clone(), index);
        self.loading.insert(path.clone());
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
            program,
            imports,
            entry,
        });

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let import_count = self.modules[index].imports.len();
        for import_index in 0..import_count {
            let import_path = self.modules[index].imports[import_index].path.clone();
            let span = self.modules[index].imports[import_index].span;
            let Some(resolved_path) = resolve_import_path(parent, &import_path) else {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!(
                        "package module `{import_path}` is not supported yet; use a relative module path"
                    ),
                ));
                continue;
            };
            let resolved_path = match resolved_path.canonicalize() {
                Ok(path) => path,
                Err(error) => {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        format!(
                            "failed to resolve module `{}`: {error}",
                            resolved_path.display()
                        ),
                    ));
                    continue;
                }
            };
            let target = self.load_module(
                resolved_path,
                format!("{key}/{import_path}"),
                false,
                Some(span),
            );
            self.modules[index].imports[import_index].target = target;
        }

        self.loading.remove(&path);
        Some(index)
    }
}

fn resolve_import_path(parent: &Path, import_path: &str) -> Option<PathBuf> {
    if !import_path.starts_with('.') {
        return None;
    }

    let mut path = parent.join(import_path);
    if path.extension().is_none() {
        path.set_extension("gust");
    }
    Some(path)
}

