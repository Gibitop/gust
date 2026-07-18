use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{
    BasicType, Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, ImportName,
    ImportNamespace, Item, MatchBranch, MatchBranchBody, Param, Pattern, Program, Stmt, StmtKind,
    StructMember, TypeRef,
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
                path: file
                    .display_path
                    .clone()
                    .unwrap_or_else(|| relative_source_path(&self.root, &file.path)),
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

        let path = file
            .display_path
            .as_deref()
            .unwrap_or_else(|| file.path.to_str().unwrap_or("<source>"));
        local_diagnostic.render(path, &source_map)
    }
}

struct SourceFile {
    path: PathBuf,
    display_path: Option<String>,
    source: String,
    offset: usize,
}

struct Package {
    root: PathBuf,
    src: PathBuf,
    project: bool,
    std: bool,
    no_std: bool,
    dependencies: HashMap<String, usize>,
}

struct Module {
    path: PathBuf,
    key: String,
    package: usize,
    program: Program,
    imports: Vec<ResolvedImport>,
    entry: bool,
}

struct ResolvedImport {
    path: String,
    names: Vec<ImportName>,
    namespace: Option<ImportNamespace>,
    glob: bool,
    exported: bool,
    weak: bool,
    span: Span,
    target: Option<usize>,
}

#[derive(Clone)]
struct Export {
    internal_name: String,
    package: usize,
    extension: bool,
}

pub fn check_project(path: &Path) -> Result<ProjectCompileResult, String> {
    check_project_with_options(path, ProjectOptions::default())
}

#[derive(Debug, Clone, Default)]
pub struct ProjectOptions {
    pub std_path: Option<PathBuf>,
    pub no_std: bool,
}

pub fn check_project_with_options(
    path: &Path,
    options: ProjectOptions,
) -> Result<ProjectCompileResult, String> {
    let requested_path = path;
    let requested_path = requested_path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve entry path `{}`: {error}",
            requested_path.display()
        )
    })?;
    let project_root = if requested_path.is_dir() && requested_path.join("project.yaml").is_file() {
        Some(requested_path.clone())
    } else if requested_path.is_file() {
        find_project_root(requested_path.parent().unwrap_or_else(|| Path::new(".")))
    } else {
        None
    };

    let (root, entry_path, root_package) = if let Some(project_root) = project_root {
        let mut packages = Vec::new();
        let root_package = load_package(&project_root, &mut packages, &mut Vec::new())?;
        let entry_path = if path.is_dir() {
            packages[root_package].src.join("main.gust")
        } else {
            requested_path
        };
        let entry_path = entry_path.canonicalize().map_err(|error| {
            format!(
                "failed to resolve entry module `{}`: {error}",
                entry_path.display()
            )
        })?;
        (project_root, entry_path, (root_package, packages))
    } else {
        let entry_path = if requested_path.is_dir() {
            requested_path.join("main.gust")
        } else {
            requested_path
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
        let packages = vec![Package {
            root: root.clone(),
            src: root.clone(),
            project: false,
            std: false,
            no_std: options.no_std,
            dependencies: HashMap::new(),
        }];
        (root, entry_path, (0, packages))
    };

    let (root_package, packages) = root_package;
    let mut loader = ProjectLoader::new(packages, options.std_path)?;
    let key = loader.module_key(root_package, &entry_path);
    loader.load_module(entry_path, key, root_package, true, None);
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

fn find_project_root(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        if ancestor.join("project.yaml").is_file() {
            return ancestor.canonicalize().ok();
        }
    }

    None
}

fn load_package(
    root: &Path,
    packages: &mut Vec<Package>,
    loading: &mut Vec<PathBuf>,
) -> Result<usize, String> {
    let root = root.canonicalize().map_err(|error| {
        format!(
            "failed to resolve Gust project `{}`: {error}",
            root.display()
        )
    })?;
    if loading.contains(&root) {
        return Err(format!(
            "cyclic package dependency reaches `{}`",
            root.display()
        ));
    }
    if let Some(index) = packages.iter().position(|package| package.root == root) {
        return Ok(index);
    }

    let manifest_path = root.join("project.yaml");
    if !manifest_path.is_file() {
        return Err(format!(
            "Gust project `{}` does not contain project.yaml",
            root.display()
        ));
    }

    let src = root.join("src");
    if !src.is_dir() {
        return Err(format!(
            "Gust project `{}` does not contain a src directory",
            root.display()
        ));
    }

    loading.push(root.clone());

    let index = packages.len();
    packages.push(Package {
        root: root.clone(),
        src: src.clone(),
        project: true,
        std: false,
        no_std: false,
        dependencies: HashMap::new(),
    });

    let manifest = read_project_manifest(&manifest_path)?;
    packages[index].no_std = manifest.no_std;
    let mut dependencies = HashMap::new();
    for (name, spec) in manifest.dependencies {
        if name == "std" {
            return Err(format!(
                "{}: dependency name `std` is reserved for the standard library",
                manifest_path.display()
            ));
        }
        let Some(path) = spec.strip_prefix("fs:") else {
            return Err(format!(
                "{}: dependency `{name}` uses unsupported source `{spec}`",
                manifest_path.display()
            ));
        };
        let dependency_root = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            root.join(path)
        };
        let dependency_index = load_package(&dependency_root, packages, loading)?;
        dependencies.insert(name, dependency_index);
    }
    packages[index].dependencies = dependencies;
    loading.pop();

    Ok(index)
}

struct ProjectManifest {
    dependencies: Vec<(String, String)>,
    no_std: bool,
}

fn read_project_manifest(path: &Path) -> Result<ProjectManifest, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
    let mut dependencies = Vec::new();
    let mut in_dependencies = false;
    let mut dependencies_indent = 0;
    let mut no_std = false;

    for (line_index, line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = line.len() - line.trim_start().len();
        if !in_dependencies {
            if indent == 0 && trimmed.starts_with("noStd:") {
                let value = trimmed
                    .strip_prefix("noStd:")
                    .expect("prefix was checked")
                    .trim();
                no_std = match unquote_yaml_scalar(value).as_str() {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(format!(
                            "{}:{line_number}: expected `noStd` to be `true` or `false`",
                            path.display()
                        ));
                    }
                };
                continue;
            }
            if indent == 0 && trimmed == "dependencies:" {
                in_dependencies = true;
                dependencies_indent = indent;
                continue;
            }
            if indent == 0 && trimmed == "dependencies: {}" {
                continue;
            }
            continue;
        }

        if indent <= dependencies_indent {
            in_dependencies = false;
            if indent == 0 && trimmed == "dependencies:" {
                in_dependencies = true;
                dependencies_indent = indent;
                continue;
            }
            if indent == 0 && trimmed == "dependencies: {}" {
                continue;
            }
        }

        if !in_dependencies {
            if indent == 0 && trimmed.starts_with("noStd:") {
                let value = trimmed
                    .strip_prefix("noStd:")
                    .expect("prefix was checked")
                    .trim();
                no_std = match unquote_yaml_scalar(value).as_str() {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(format!(
                            "{}:{line_number}: expected `noStd` to be `true` or `false`",
                            path.display()
                        ));
                    }
                };
            }
            continue;
        }

        let Some((name, value)) = trimmed.split_once(':') else {
            return Err(format!(
                "{}:{line_number}: expected `name: fs:path` dependency entry",
                path.display()
            ));
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            return Err(format!(
                "{}:{line_number}: expected `name: fs:path` dependency entry",
                path.display()
            ));
        }
        if dependencies.iter().any(|(existing, _)| existing == name) {
            return Err(format!(
                "{}:{line_number}: duplicate dependency `{name}`",
                path.display()
            ));
        }
        dependencies.push((name.to_string(), unquote_yaml_scalar(value)));
    }

    Ok(ProjectManifest {
        dependencies,
        no_std,
    })
}

fn unquote_yaml_scalar(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        return value[1..value.len() - 1].to_string();
    }

    value.to_string()
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
