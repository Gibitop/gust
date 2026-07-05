use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

use gustc::c_codegen::emit_c;
use gustc::diagnostic::Severity;
use gustc::lower::lower_program;
use gustc::project::check_project;

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

#[test]
fn relative_modules_link_functions_types_and_private_dependencies() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./lib/greeting import greet

fn main() {
    let value: greet.Greeter = greet.Greeter.new("Gust")
    io.println(greet.greeting(value))
    let message = match greet.mood() {
        greet.Mood.Happy => "happy",
    }
    io.println(message)
}"#,
    );
    project.write(
        "lib/greeting.gust",
        r#"struct Greeter {
    name: String

    static fn new(name: String): Self => Self { name: name }
}

fn punctuation(): String => "!"

fn greeting(value: Greeter): String {
    return "Hello, " + value.name + punctuation()
}

enum Mood {
    Happy
}

fn mood(): Mood => Mood.Happy"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected project to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("project should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("greeting"));
    assert!(source.contains("punctuation"));
}

#[test]
fn directory_entry_uses_main_gust() {
    let project = TempProject::new();
    project.write("main.gust", "fn main() {}");

    let result = check_project(&project.path).expect("project directory should load");

    assert!(result.diagnostics.is_empty());
}

#[test]
fn unimported_names_are_not_visible() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./helper import { visible }

fn main() {
    io.println(hidden())
}"#,
    );
    project.write(
        "helper.gust",
        r#"fn visible(): String => "visible"
fn hidden(): String => "hidden""#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `hidden`")
        }),
        "expected hidden function error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn missing_exports_are_reported_at_the_import() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./helper import { missing }

fn main() {}"#,
    );
    project.write("helper.gust", "fn available() {}");

    let result = check_project(&project.path("main.gust")).expect("project should load");
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("does not export `missing`"))
        .expect("missing export diagnostic should be present");
    let rendered = result.sources.render(diagnostic);

    assert!(rendered.contains(path_suffix("main.gust")));
    assert!(rendered.contains(":1:24: error:"));
}

#[test]
fn imported_module_diagnostics_use_the_imported_file() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./helper import { broken }

fn main() {}"#,
    );
    project.write(
        "helper.gust",
        r#"fn broken(): String {
    return 1
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .message
                .contains("expected value of type `String`")
        })
        .unwrap_or_else(|| {
            panic!(
                "return type diagnostic should be present, got {:?}",
                result.diagnostics
            )
        });
    let rendered = result.sources.render(diagnostic);

    assert!(rendered.contains(path_suffix("helper.gust")));
}

#[test]
fn import_cycles_are_rejected() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./a import { value }

fn main() {}"#,
    );
    project.write(
        "a.gust",
        r#"from ./b import { other }

fn value(): String => other()"#,
    );
    project.write(
        "b.gust",
        r#"from ./a import { value }

fn other(): String => value()"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("module import cycle"))
    );
}

#[test]
fn extensions_require_named_imports_and_preserve_member_precedence() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./extensions import { Greeter, withSuffix as suffix }

fn main() {
    let greeter = Greeter { name: "Gust" }
    io.println(greeter.label())
    io.println("Gust".suffix("!"))
}"#,
    );
    project.write(
        "extensions.gust",
        r#"struct Greeter {
    name: String

    fn label(): String => "member"
}

fn Greeter.label(): String => "extension"
fn String.withSuffix(suffix: String): String => self + suffix"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected imported extension to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("imported extension should lower");
}

#[test]
fn extensions_do_not_leak_from_loaded_modules() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./extensions import { marker }

fn main() {
    io.println("Gust".withSuffix("!"))
}"#,
    );
    project.write(
        "extensions.gust",
        r#"fn marker(): String => "marker"
fn String.withSuffix(suffix: String): String => self + suffix"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    let diagnostics =
        lower_program(&result.program).expect_err("unimported extension should not lower");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown method `withSuffix`"))
    );
}

#[test]
fn duplicate_import_aliases_are_rejected() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./helper import { first as value, second as value }

fn main() {}"#,
    );
    project.write(
        "helper.gust",
        r#"fn first(): String => "first"
fn second(): String => "second""#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("imported name `value` conflicts")
    }));
}

#[test]
fn unknown_namespace_members_are_rejected() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./helper import helper

fn main() {
    helper.missing()
}"#,
    );
    project.write("helper.gust", "fn available() {}");

    let result = check_project(&project.path("main.gust")).expect("project should load");

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("module namespace `helper` does not export `missing`")
    }));
}

#[test]
fn imported_generic_structs_are_monomorphized_after_module_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./box import { Box }

fn main() {
    let value = Box.new("from module")
    io.println(value.get())
}"#,
    );
    project.write(
        "box.gust",
        r#"struct Box<T> {
    value: T

    static fn new(value: T): Self => Self { value: value }

    fn get(): T {
        return self.value
    }
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected imported generic struct to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("imported generic struct should lower");
}

fn path_suffix(path: &str) -> &str {
    Path::new(path).to_str().expect("test path should be UTF-8")
}
