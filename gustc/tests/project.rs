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
fn root_standard_library_modules_link_through_relative_imports() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../std/option.gust"));
    project.write("std/iter.gust", include_str!("../../std/iter.gust"));
    project.write(
        "examples/main.gust",
        r#"from ../std/iter import { Iterator }
from ../std/option import { Option }

struct Counter {
    value: i32
}

impl Iterator<i32> for Counter {
    fn next(mut self): Option<i32> {
        let value = self.value
        self.value++
        return Option.Some(value)
    }
}

fn main() {
    let mut iterator: Iterator<i32> = Counter { value: 1 }
    let message = match iterator.next() {
        Option.Some(value) => value.toString(),
        Option.None => "empty",
    }
    io.println(message)
}"#,
    );

    let result = check_project(&project.path("examples/main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected root standard library modules to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("root standard library modules should lower");
}

#[test]
fn enum_methods_survive_project_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"enum Option<T> {
    Some(T)
    None

    fn unwrapOr(fallback: T): T {
        return match self {
            Option.Some(value) => value,
            Option.None => fallback,
        }
    }
}

fn main() {
    let present = Option.Some(42)
    io.println(present.unwrapOr(0).toString())
}
"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected enum methods to validate after linking, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("linked enum methods should lower");
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

#[test]
fn imported_generic_enums_are_monomorphized_after_module_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./option import { Option, none }

fn main() {
    let some = Option.Some("from module")
    let missing = none()
    let first = match some {
        Option.Some(value) => value,
        Option.None => "missing",
    }
    let second = match missing {
        Option.Some(value) => value,
        Option.None => "missing",
    }
    io.println(first + second)
}"#,
    );
    project.write(
        "option.gust",
        r#"enum Option<T> {
    Some(T)
    None
}

fn none(): Option<String> => Option.None"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected imported generic enum to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("imported generic enum should lower");
}

#[test]
fn imported_generic_functions_are_monomorphized_after_module_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./identity import { identity }

fn main() {
    io.println(identity("from module"))
}"#,
    );
    project.write("identity.gust", r#"fn identity<T>(value: T): T => value"#);

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected imported generic function to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("imported generic function should lower");
}

#[test]
fn imported_generic_traits_are_monomorphized_after_module_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./named import { Named, Person }

fn printName(value: Named<String>) {
    io.println(value.name())
}

fn main() {
    let person = Person.new("from module")
    let named: Named<String> = person
    printName(person)
    io.println(named.name())
}"#,
    );
    project.write(
        "named.gust",
        r#"trait Named<T> {
    fn name(): T
}

struct Person {
    name: String

    static fn new(name: String): Self => Self { name: name }
}

impl Named<String> for Person {
    fn name() => self.name
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected imported generic trait to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("imported generic trait should lower");
}

#[test]
fn imported_generic_trait_impl_templates_are_monomorphized_after_module_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./named import { Named, Box }

fn main() {
    let value = Box.new("from module")
    let named: Named<String> = value
    io.println(named.name())
}"#,
    );
    project.write(
        "named.gust",
        r#"trait Named<T> {
    fn name(): T
}

struct Box<T> {
    value: T

    static fn new(value: T): Self => Self { value: value }
}

impl<T> Named<T> for Box<T> {
    fn name() => self.value
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected imported generic trait impl template to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("imported generic trait impl template should lower");
}

#[test]
fn overlapping_trait_impls_are_rejected_across_modules() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./first import { first }
from ./second import { second }

fn main() {
    first()
    second()
}"#,
    );
    project.write(
        "model.gust",
        r#"trait Describe {
    fn describe(): String
}

struct Person {
    name: String
}"#,
    );
    project.write(
        "first.gust",
        r#"from ./model import { Describe }

impl<T> Describe for T {
    fn describe() => "value"
}

fn first() {}"#,
    );
    project.write(
        "second.gust",
        r#"from ./model import { Describe, Person }

impl Describe for Person {
    fn describe() => self.name
}

fn second() {}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting implementations of trait")
    }));
}

fn path_suffix(path: &str) -> &str {
    Path::new(path).to_str().expect("test path should be UTF-8")
}
