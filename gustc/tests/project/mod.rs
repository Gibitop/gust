use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::atomic::{AtomicUsize, Ordering};

use gustc::c_codegen::{CCodegenOptions, emit_c, emit_c_with_options};
use gustc::diagnostic::Severity;
use gustc::lower::{lower_program, lower_program_with_source_files};
use gustc::project::{ProjectOptions, check_project, check_project_with_options};

static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

struct TempProject {
    path: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let id = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("gust-project-{}-{id}", process::id()));
        fs::create_dir_all(&path).expect("temporary project directory should be created");
        Self { path }
    }

    fn write(&self, path: &str, source: &str) {
        let path = self.path.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("module directory should be created");
        }
        fs::write(path, source).expect("module source should be written");
    }

    fn path(&self, path: &str) -> PathBuf {
        self.path.join(path)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn check_project_no_std(path: &Path) -> Result<gustc::project::ProjectCompileResult, String> {
    check_project_with_options(
        path,
        ProjectOptions {
            std_path: None,
            no_std: true,
        },
    )
}

include!("modules.rs");
include!("std.rs");
include!("generics.rs");
include!("diagnostics.rs");
include!("comptime.rs");
include!("helpers.rs");
