struct ProjectLoader {
    modules: Vec<Module>,
    module_indexes: HashMap<(usize, PathBuf), usize>,
    packages: Vec<Package>,
    std_package: Option<usize>,
    sources: Vec<SourceFile>,
    diagnostics: Vec<Diagnostic>,
    next_offset: usize,
    load_error: Option<String>,
}

impl ProjectLoader {
    fn new(mut packages: Vec<Package>, std_path: Option<PathBuf>) -> Result<Self, String> {
        let std_package = if packages.iter().any(|package| !package.no_std) {
            Some(load_std_package(&mut packages, std_path)?)
        } else {
            None
        };

        Ok(Self {
            modules: Vec::new(),
            module_indexes: HashMap::new(),
            packages,
            std_package,
            sources: Vec::new(),
            diagnostics: Vec::new(),
            next_offset: 0,
            load_error: None,
        })
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
            display_path: self.source_display_path(package, &path),
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
        let imports = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Import(import) => Some(ResolvedImport {
                    path: import.path.clone(),
                    names: import.names.clone(),
                    namespace: import.namespace.clone(),
                    glob: import.glob,
                    exported: import.exported,
                    weak: false,
                    span: import.span,
                    target: None,
                }),
                _ => None,
            })
            .collect();
        let mut imports: Vec<ResolvedImport> = imports;
        if let Some(std_package) = self.std_package
            && !self.packages[package].no_std
            && package != std_package
        {
            imports.push(ResolvedImport {
                path: "std/prelude".to_string(),
                names: Vec::new(),
                namespace: None,
                glob: true,
                exported: false,
                weak: true,
                span: Span::new(offset, offset),
                target: None,
            });
        }
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
        if dependency_name == "std" {
            if self.packages[package].no_std {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    "`std` is unavailable because this package has `noStd: true`",
                ));
                return None;
            }
            let Some(std_package) = self.std_package else {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    "standard library package is not configured",
                ));
                return None;
            };
            let mut path = self.packages[std_package].src.clone();
            if module_path.is_empty() {
                path.push("lib");
            } else {
                path.push(module_path);
            }
            if path.extension().is_none() {
                path.set_extension("gust");
            }
            let path = self.canonicalize_import_path(path, span)?;
            if !path.starts_with(&self.packages[std_package].src) {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!(
                        "standard-library import `{import_path}` resolves outside standard-library source directory `{}`",
                        self.packages[std_package].src.display()
                    ),
                ));
                return None;
            }
            return Some((std_package, path));
        }
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

    fn source_display_path(&self, package: usize, path: &Path) -> Option<String> {
        let package = &self.packages[package];
        if !package.std {
            return None;
        }

        let module_path = path.strip_prefix(&package.src).unwrap_or(path);
        Some(format!("std/{}", module_path.to_string_lossy()))
    }
}

fn load_std_package(
    packages: &mut Vec<Package>,
    std_path: Option<PathBuf>,
) -> Result<usize, String> {
    let root = resolve_std_path(std_path)?;
    if let Some(index) = packages.iter().position(|package| package.root == root) {
        packages[index].std = true;
        packages[index].no_std = true;
        return Ok(index);
    }

    let manifest_path = root.join("project.yaml");
    if !manifest_path.is_file() {
        return Err(format!(
            "standard library project `{}` does not contain project.yaml",
            root.display()
        ));
    }

    let src = root.join("src");
    if !src.is_dir() {
        return Err(format!(
            "standard library project `{}` does not contain a src directory",
            root.display()
        ));
    }

    let index = packages.len();
    packages.push(Package {
        root,
        src,
        project: true,
        std: true,
        no_std: true,
        dependencies: HashMap::new(),
        aliases: HashSet::new(),
        comptime_permissions: ComptimePermissions::default(),
    });
    Ok(index)
}

fn resolve_std_path(std_path: Option<PathBuf>) -> Result<PathBuf, String> {
    let candidates = if let Some(path) = std_path {
        vec![path]
    } else {
        let mut paths = Vec::new();
        if let Ok(executable) = std::env::current_exe()
            && let Some(executable_dir) = executable.parent()
        {
            paths.push(executable_dir.join("../std"));
        }
        paths.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../std"));
        paths
    };

    for path in candidates {
        if let Ok(path) = path.canonicalize()
            && path.join("project.yaml").is_file()
            && path.join("src").is_dir()
        {
            return Ok(path);
        }
    }

    Err("failed to locate standard library; pass `--std-path <path>`".to_string())
}
