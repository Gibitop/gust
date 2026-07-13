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
use crate::lower::LoweringSourceFile;
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
    root: PathBuf,
}

impl ProjectSources {
    pub fn to_lowering_source_files(&self) -> Vec<LoweringSourceFile> {
        self.files
            .iter()
            .map(|file| LoweringSourceFile {
                path: relative_source_path(&self.root, &file.path),
                source: file.source.clone(),
                offset: file.offset,
            })
            .collect()
    }

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
    let root = entry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
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
            root,
        },
    })
}

fn relative_source_path(root: &Path, path: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(root) {
        return relative.to_string_lossy().into_owned();
    }

    let mut prefix = PathBuf::new();
    for ancestor in root.ancestors() {
        if let Ok(relative) = path.strip_prefix(ancestor) {
            let suffix = relative.to_string_lossy();
            if suffix.is_empty() {
                return prefix.to_string_lossy().into_owned();
            }
            if prefix.as_os_str().is_empty() {
                return suffix.into_owned();
            }
            return prefix.join(relative).to_string_lossy().into_owned();
        }
        prefix.push("..");
    }

    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

include!("loader.rs");
include!("link.rs");
include!("rewrite.rs");
include!("spans.rs");
