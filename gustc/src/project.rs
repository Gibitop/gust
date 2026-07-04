use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{
    Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, ImportName, ImportNamespace,
    Item, MatchBranch, MatchBranchBody, Param, Pattern, Program, Stmt, StmtKind, StructMember,
    TypeRef,
};
use crate::diagnostic::{Diagnostic, Severity};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::semantic::validate;
use crate::span::{SourceMap, Span};

pub struct ProjectCompileResult {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
    pub sources: ProjectSources,
}

impl ProjectCompileResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

pub struct ProjectSources {
    files: Vec<SourceFile>,
}

impl ProjectSources {
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let file = self
            .files
            .iter()
            .rev()
            .find(|file| diagnostic.span.start >= file.offset)
            .unwrap_or_else(|| &self.files[0]);
        let local_span = Span::new(
            diagnostic.span.start.saturating_sub(file.offset),
            diagnostic.span.end.saturating_sub(file.offset),
        );
        let local_diagnostic = Diagnostic {
            severity: diagnostic.severity,
            span: local_span,
            message: diagnostic.message.clone(),
        };
        let source_map = SourceMap::new(&file.source);

        local_diagnostic.render(&file.path.to_string_lossy(), &source_map)
    }
}

struct SourceFile {
    path: PathBuf,
    source: String,
    offset: usize,
}

struct Module {
    path: PathBuf,
    key: String,
    program: Program,
    imports: Vec<ResolvedImport>,
    entry: bool,
}

struct ResolvedImport {
    path: String,
    names: Vec<ImportName>,
    namespace: Option<ImportNamespace>,
    span: Span,
    target: Option<usize>,
}

#[derive(Clone)]
struct Export {
    internal_name: String,
    extension: bool,
}

pub fn check_project(path: &Path) -> Result<ProjectCompileResult, String> {
    let entry_path = if path.is_dir() {
        path.join("main.gust")
    } else {
        path.to_path_buf()
    };
    let entry_path = entry_path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve entry module `{}`: {error}",
            entry_path.display()
        )
    })?;
    let mut loader = ProjectLoader::new();
    loader.load_module(entry_path, "<entry>".to_string(), true, None);
    if let Some(error) = loader.load_error {
        return Err(error);
    }

    let mut diagnostics = loader.diagnostics;
    let program = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        Program { items: Vec::new() }
    } else {
        link_modules(&loader.modules, &mut diagnostics)
    };

    if !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        diagnostics.extend(validate(&program));
    }

    Ok(ProjectCompileResult {
        program,
        diagnostics,
        sources: ProjectSources {
            files: loader.sources,
        },
    })
}

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

fn link_modules(modules: &[Module], diagnostics: &mut Vec<Diagnostic>) -> Program {
    let mut exports = Vec::with_capacity(modules.len());
    let mut local_names = Vec::with_capacity(modules.len());
    let mut local_extensions = Vec::with_capacity(modules.len());

    for module in modules {
        let mut module_exports = HashMap::new();
        let mut module_names = HashMap::new();
        let mut module_extensions = HashMap::new();

        for item in &module.program.items {
            let Some((name, extension, span)) = item_export(item) else {
                continue;
            };
            let internal_name = if extension {
                qualified_extension_name(&module.key, name)
            } else if module.entry {
                name.to_string()
            } else {
                qualified_name(&module.key, name)
            };
            let export = Export {
                internal_name: internal_name.clone(),
                extension,
            };
            if let Some(previous) = module_exports.insert(name.to_string(), export)
                && !(previous.extension && extension)
            {
                diagnostics.push(Diagnostic::error(
                    span,
                    format!("duplicate top-level name `{name}` in this module"),
                ));
            }
            if extension {
                if name == "clone" {
                    diagnostics.push(Diagnostic::error(
                        span,
                        "extension function name `clone` is reserved for the built-in deep clone operation",
                    ));
                }
                module_extensions.insert(name.to_string(), internal_name);
            } else {
                module_names.insert(name.to_string(), internal_name);
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

            if let Some(namespace) = &import.namespace {
                if visible_names[module_index].contains_key(&namespace.name)
                    || visible_namespaces[module_index]
                        .insert(namespace.name.clone(), target)
                        .is_some()
                {
                    diagnostics.push(Diagnostic::error(
                        namespace.span,
                        format!(
                            "module namespace `{}` conflicts with another name in this module",
                            namespace.name
                        ),
                    ));
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

fn item_export(item: &Item) -> Option<(&str, bool, Span)> {
    match item {
        Item::Enum(item) => Some((&item.name, false, item.span)),
        Item::Struct(item) => Some((&item.name, false, item.span)),
        Item::Function(item) => item.name.as_deref().map(|name| (name, false, item.span)),
        Item::Extension(item) => item
            .function
            .name
            .as_deref()
            .map(|name| (name, true, item.span)),
        Item::Import(_) => None,
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

struct ModuleRewriter<'names, 'diagnostics> {
    local_names: &'names HashMap<String, String>,
    visible_names: &'names HashMap<String, String>,
    local_extensions: &'names HashMap<String, String>,
    visible_extensions: &'names HashMap<String, String>,
    visible_namespaces: &'names HashMap<String, usize>,
    exports: &'names [HashMap<String, Export>],
    diagnostics: &'diagnostics mut Vec<Diagnostic>,
    scopes: Vec<HashSet<String>>,
    entry: bool,
}

impl<'names, 'diagnostics> ModuleRewriter<'names, 'diagnostics> {
    fn new(
        local_names: &'names HashMap<String, String>,
        visible_names: &'names HashMap<String, String>,
        local_extensions: &'names HashMap<String, String>,
        visible_extensions: &'names HashMap<String, String>,
        visible_namespaces: &'names HashMap<String, usize>,
        exports: &'names [HashMap<String, Export>],
        diagnostics: &'diagnostics mut Vec<Diagnostic>,
        entry: bool,
    ) -> Self {
        Self {
            local_names,
            visible_names,
            local_extensions,
            visible_extensions,
            visible_namespaces,
            exports,
            diagnostics,
            scopes: Vec::new(),
            entry,
        }
    }

    fn rewrite_item(&mut self, item: &mut Item) {
        match item {
            Item::Enum(item) => {
                self.rewrite_declared_name(&mut item.name);
                for variant in &mut item.variants {
                    if let Some(type_ref) = &mut variant.payload {
                        self.rewrite_type(type_ref);
                    }
                }
            }
            Item::Struct(item) => {
                self.rewrite_declared_name(&mut item.name);
                for member in &mut item.members {
                    match member {
                        StructMember::Field(field) => self.rewrite_type(&mut field.type_ref),
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            self.rewrite_function(function);
                        }
                    }
                }
            }
            Item::Function(function) => {
                if let Some(name) = &mut function.name
                    && !(self.entry && name == "main")
                {
                    self.rewrite_declared_name(name);
                }
                self.rewrite_function(function);
            }
            Item::Extension(extension) => {
                self.rewrite_type(&mut extension.type_ref);
                if let Some(name) = &mut extension.function.name
                    && let Some(internal_name) = self.local_extensions.get(name)
                {
                    *name = internal_name.clone();
                }
                self.rewrite_function(&mut extension.function);
            }
            Item::Import(_) => {}
        }
    }

    fn rewrite_declared_name(&self, name: &mut String) {
        if let Some(internal_name) = self.local_names.get(name) {
            *name = internal_name.clone();
        }
    }

    fn rewrite_function(&mut self, function: &mut FunctionDecl) {
        for param in &mut function.params {
            if let Some(type_ref) = &mut param.type_ref {
                self.rewrite_type(type_ref);
            }
        }
        if let Some(return_type) = &mut function.return_type {
            self.rewrite_type(return_type);
        }

        self.scopes.push(
            function
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect(),
        );
        match &mut function.body {
            FunctionBody::Block(block) => self.rewrite_block(block),
            FunctionBody::Expr(expr) => self.rewrite_expr(expr),
        }
        self.scopes.pop();
    }

    fn rewrite_block(&mut self, block: &mut Block) {
        self.scopes.push(HashSet::new());
        for statement in &mut block.statements {
            self.rewrite_statement(statement);
        }
        self.scopes.pop();
    }

    fn rewrite_statement(&mut self, statement: &mut Stmt) {
        match &mut statement.kind {
            StmtKind::Let {
                name,
                type_annotation,
                value,
                ..
            } => {
                if let Some(type_ref) = type_annotation {
                    self.rewrite_type(type_ref);
                }
                if let Some(value) = value {
                    self.rewrite_expr(value);
                }
                self.define_local(name);
            }
            StmtKind::Assign { target, value, .. } => {
                self.rewrite_expr(target);
                self.rewrite_expr(value);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    self.rewrite_expr(value);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.rewrite_expr(condition);
                self.rewrite_block(then_branch);
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.rewrite_block(block),
                        ElseBranch::If(statement) => self.rewrite_statement(statement),
                    }
                }
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                self.rewrite_expr(iterable);
                self.scopes.push(HashSet::from([name.clone()]));
                for statement in &mut body.statements {
                    self.rewrite_statement(statement);
                }
                self.scopes.pop();
            }
            StmtKind::Expr(expr) => self.rewrite_expr(expr),
        }
    }

    fn rewrite_expr(&mut self, expr: &mut Expr) {
        if let ExprKind::Member { object, name } = &expr.kind
            && let ExprKind::Identifier(namespace) = &object.kind
            && let Some(internal_name) = self.resolve_namespace_member(namespace, name, expr.span)
        {
            expr.kind = ExprKind::Identifier(internal_name);
            return;
        }

        match &mut expr.kind {
            ExprKind::Identifier(name) => {
                if !self.is_local(name)
                    && let Some(internal_name) = self.visible_names.get(name)
                {
                    *name = internal_name.clone();
                }
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.rewrite_expr(item);
                }
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Member { name, .. } = &mut callee.kind
                    && let Some(internal_name) = self.visible_extensions.get(name)
                {
                    *name = internal_name.clone();
                }
                self.rewrite_expr(callee);
                for arg in args {
                    self.rewrite_expr(arg);
                }
            }
            ExprKind::Member { object, .. } => self.rewrite_expr(object),
            ExprKind::StructInit { name, fields } => {
                if let Some(internal_name) = self.resolve_qualified_name(name, expr.span) {
                    *name = internal_name;
                } else if let Some(internal_name) = self.visible_names.get(name) {
                    *name = internal_name.clone();
                }
                for field in fields {
                    self.rewrite_expr(&mut field.value);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.rewrite_expr(left);
                self.rewrite_expr(right);
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.rewrite_expr(operand);
            }
            ExprKind::Match { value, branches } => {
                self.rewrite_expr(value);
                for branch in branches {
                    self.rewrite_match_branch(branch);
                }
            }
            ExprKind::Lambda(function) => self.rewrite_function(function),
            ExprKind::Number(_) | ExprKind::String(_) | ExprKind::Bool(_) | ExprKind::Missing => {}
        }
    }

    fn rewrite_match_branch(&mut self, branch: &mut MatchBranch) {
        let binding = match &mut branch.pattern {
            Pattern::Variant {
                enum_name, binding, ..
            } => {
                if let Some(internal_name) = self.resolve_qualified_name(enum_name, branch.span) {
                    *enum_name = internal_name;
                } else if let Some(internal_name) = self.visible_names.get(enum_name) {
                    *enum_name = internal_name.clone();
                }
                binding.clone()
            }
            Pattern::String { .. } | Pattern::Wildcard { .. } => None,
        };

        self.scopes
            .push(binding.into_iter().collect::<HashSet<_>>());
        match &mut branch.body {
            MatchBranchBody::Expr(expr) => self.rewrite_expr(expr),
            MatchBranchBody::Block(block) => self.rewrite_block(block),
        }
        self.scopes.pop();
    }

    fn rewrite_type(&self, type_ref: &mut TypeRef) {
        if let Some(internal_name) =
            resolve_qualified_name(self.visible_namespaces, self.exports, &type_ref.name)
        {
            type_ref.name = internal_name;
        } else if let Some(internal_name) = self.visible_names.get(&type_ref.name) {
            type_ref.name = internal_name.clone();
        }
        for arg in &mut type_ref.args {
            self.rewrite_type(arg);
        }
    }

    fn define_local(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn resolve_qualified_name(&mut self, name: &str, span: Span) -> Option<String> {
        let (namespace, member) = name.split_once('.')?;
        self.resolve_namespace_member(namespace, member, span)
    }

    fn resolve_namespace_member(
        &mut self,
        namespace: &str,
        member: &str,
        span: Span,
    ) -> Option<String> {
        let target = self.visible_namespaces.get(namespace)?;
        let Some(export) = self.exports[*target].get(member) else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("module namespace `{namespace}` does not export `{member}`"),
            ));
            return None;
        };
        if export.extension {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "extension function `{member}` must be imported by name to participate in method lookup"
                ),
            ));
            return None;
        }
        Some(export.internal_name.clone())
    }
}

fn resolve_qualified_name(
    visible_namespaces: &HashMap<String, usize>,
    exports: &[HashMap<String, Export>],
    name: &str,
) -> Option<String> {
    let (namespace, member) = name.split_once('.')?;
    let target = visible_namespaces.get(namespace)?;
    let export = exports[*target].get(member)?;
    (!export.extension).then(|| export.internal_name.clone())
}

fn shift_diagnostic(mut diagnostic: Diagnostic, offset: usize) -> Diagnostic {
    shift_span(&mut diagnostic.span, offset);
    diagnostic
}

fn shift_program(program: &mut Program, offset: usize) {
    for item in &mut program.items {
        match item {
            Item::Import(item) => {
                shift_span(&mut item.span, offset);
                for name in &mut item.names {
                    shift_span(&mut name.span, offset);
                }
                if let Some(namespace) = &mut item.namespace {
                    shift_span(&mut namespace.span, offset);
                }
            }
            Item::Enum(item) => {
                shift_span(&mut item.span, offset);
                for variant in &mut item.variants {
                    shift_span(&mut variant.span, offset);
                    if let Some(type_ref) = &mut variant.payload {
                        shift_type(type_ref, offset);
                    }
                }
            }
            Item::Struct(item) => {
                shift_span(&mut item.span, offset);
                for member in &mut item.members {
                    match member {
                        StructMember::Field(field) => {
                            shift_span(&mut field.span, offset);
                            shift_type(&mut field.type_ref, offset);
                        }
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            shift_function(function, offset);
                        }
                    }
                }
            }
            Item::Extension(item) => {
                shift_span(&mut item.span, offset);
                shift_type(&mut item.type_ref, offset);
                shift_function(&mut item.function, offset);
            }
            Item::Function(function) => shift_function(function, offset),
        }
    }
}

fn shift_function(function: &mut FunctionDecl, offset: usize) {
    shift_span(&mut function.span, offset);
    for param in &mut function.params {
        shift_param(param, offset);
    }
    if let Some(return_type) = &mut function.return_type {
        shift_type(return_type, offset);
    }
    match &mut function.body {
        FunctionBody::Block(block) => shift_block(block, offset),
        FunctionBody::Expr(expr) => shift_expr(expr, offset),
    }
}

fn shift_param(param: &mut Param, offset: usize) {
    shift_span(&mut param.span, offset);
    if let Some(type_ref) = &mut param.type_ref {
        shift_type(type_ref, offset);
    }
}

fn shift_type(type_ref: &mut TypeRef, offset: usize) {
    shift_span(&mut type_ref.span, offset);
    for arg in &mut type_ref.args {
        shift_type(arg, offset);
    }
}

fn shift_block(block: &mut Block, offset: usize) {
    shift_span(&mut block.span, offset);
    for statement in &mut block.statements {
        shift_statement(statement, offset);
    }
}

fn shift_statement(statement: &mut Stmt, offset: usize) {
    shift_span(&mut statement.span, offset);
    match &mut statement.kind {
        StmtKind::Let {
            type_annotation,
            value,
            ..
        } => {
            if let Some(type_ref) = type_annotation {
                shift_type(type_ref, offset);
            }
            if let Some(value) = value {
                shift_expr(value, offset);
            }
        }
        StmtKind::Assign { target, value, .. } => {
            shift_expr(target, offset);
            shift_expr(value, offset);
        }
        StmtKind::Return { value } => {
            if let Some(value) = value {
                shift_expr(value, offset);
            }
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            shift_expr(condition, offset);
            shift_block(then_branch, offset);
            if let Some(else_branch) = else_branch {
                match else_branch {
                    ElseBranch::Block(block) => shift_block(block, offset),
                    ElseBranch::If(statement) => shift_statement(statement, offset),
                }
            }
        }
        StmtKind::For { iterable, body, .. } => {
            shift_expr(iterable, offset);
            shift_block(body, offset);
        }
        StmtKind::Expr(expr) => shift_expr(expr, offset),
    }
}

fn shift_expr(expr: &mut Expr, offset: usize) {
    shift_span(&mut expr.span, offset);
    match &mut expr.kind {
        ExprKind::Array(items) => {
            for item in items {
                shift_expr(item, offset);
            }
        }
        ExprKind::Call { callee, args } => {
            shift_expr(callee, offset);
            for arg in args {
                shift_expr(arg, offset);
            }
        }
        ExprKind::Member { object, .. } => shift_expr(object, offset),
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                shift_span(&mut field.span, offset);
                shift_expr(&mut field.value, offset);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            shift_expr(left, offset);
            shift_expr(right, offset);
        }
        ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
            shift_expr(operand, offset);
        }
        ExprKind::Match { value, branches } => {
            shift_expr(value, offset);
            for branch in branches {
                shift_span(&mut branch.span, offset);
                match &mut branch.pattern {
                    Pattern::Variant { span, .. }
                    | Pattern::String { span, .. }
                    | Pattern::Wildcard { span } => shift_span(span, offset),
                }
                match &mut branch.body {
                    MatchBranchBody::Expr(expr) => shift_expr(expr, offset),
                    MatchBranchBody::Block(block) => shift_block(block, offset),
                }
            }
        }
        ExprKind::Lambda(function) => shift_function(function, offset),
        ExprKind::Identifier(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn shift_span(span: &mut Span, offset: usize) {
    span.start += offset;
    span.end += offset;
}
