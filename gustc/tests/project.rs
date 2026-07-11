use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
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
fn string_intrinsics_are_available_without_imports() {
    let project = TempProject::new();
    project.write(
        "examples/main.gust",
        r#"fn main() {
    let value = "Gust"
    if value.byteLen() == 4 && !value.isEmpty() {
        io.println(value)
    }
}"#,
    );

    let result = check_project(&project.path("examples/main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected intrinsic string operations to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("intrinsic string operations should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("gust_value.gust_byte_len"), "{source}");
    assert!(source.contains("gust_value.gust_byte_len == 0"), "{source}");
}

#[test]
fn string_builder_uses_growable_runtime_storage() {
    let project = TempProject::new();
    project.write(
        "std/string-builder.gust",
        include_str!("../../std/string-builder.gust"),
    );
    project.write(
        "examples/main.gust",
        r#"from ../std/string-builder import { StringBuilder }

fn main() {
    let mut builder = StringBuilder.withCapacity(1)
    builder.append("hello")
    builder.append(" world")
    io.println(builder.build())
}"#,
    );

    let result = check_project(&project.path("examples/main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected StringBuilder to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("StringBuilder should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("gust_rt_string_builder_append_"));
    assert!(source.contains("gust_rt_string_builder_build_"));
}

#[test]
fn collection_literals_lower_through_from_elements() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../std/option.gust"));
    project.write("std/iter.gust", include_str!("../../std/iter.gust"));
    project.write(
        "std/collection.gust",
        include_str!("../../std/collection.gust"),
    );
    project.write(
        "std/raw-buffer.gust",
        include_str!("../../std/raw-buffer.gust"),
    );
    project.write(
        "std/array-list.gust",
        include_str!("../../std/array-list.gust"),
    );
    project.write(
        "main.gust",
        r#"from ./std/array-list import { ArrayList }
from ./std/collection import { FromElements }

struct TestCollection<T> {
    values: ArrayList<T>
}

impl<T> FromElements<T> for TestCollection<T> {
    static fn withElementCapacity(capacity: usize): Self => TestCollection<T> {
        values: ArrayList<T>.withCapacity(capacity),
    }

    fn add(mut self, value: T): void {
        self.values.push(value)
    }
}

fn main() {
    let mut values = [1, 2, 3]
    values.push(4)
    values.set(1, 20)
    let popped = values.pop()
    let custom: TestCollection<i32> = [5, 6]
    io.println(values.len().toString())
    let iterator = values.iterator()
    let copied = ArrayList.fromIterator(iterator)
    io.println(copied.len().toString())

    for value in copied {
        io.println(value.toString())
    }
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected collection project to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("collection project should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("gust_collection"));
    assert!(source.contains("gust_data"));
    let c_path = project.path("collection.c");
    fs::write(&c_path, source).expect("generated C should be written");
    let output = Command::new("cc")
        .arg("-fsyntax-only")
        .arg(&c_path)
        .output()
        .expect("C compiler should run");
    assert!(
        output.status.success(),
        "generated collection C should compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let executable = project.path("collection");
    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build collection executable");
    assert!(
        output.status.success(),
        "generated collection C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable)
        .output()
        .expect("collection executable should run");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n3\n1\n20\n3\n");
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

#[test]
fn imported_generic_inference_diagnostics_use_gust_names() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./collection import { ArrayList }

fn main() {
    let values = ArrayList.empty()
}"#,
    );
    project.write(
        "collection.gust",
        r#"struct ArrayList<T> {
    marker: i32

    static fn empty(): Self => Self { marker: 0 }
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("cannot infer type arguments"))
        .expect("generic inference should report a diagnostic");

    assert!(
        diagnostic
            .message
            .contains("generic static call `ArrayList.empty`")
    );
    assert_gust_diagnostic_name(&diagnostic.message);
    assert_rendered_at(&result, diagnostic, &project.path("main.gust"), 4, 18);
}

#[test]
fn imported_method_and_static_method_diagnostics_use_gust_names() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./model import { Gadget }

fn main() {
    let gadget = Gadget {}
    gadget.missing()
    Gadget.missingStatic()
}"#,
    );
    project.write("model.gust", "struct Gadget {}");

    let result = check_project(&project.path("main.gust")).expect("project should load");
    let method = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("unknown method `missing`"))
        .expect("missing method should report a diagnostic");
    let static_method = result
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .message
                .contains("unknown static function `missingStatic`")
        })
        .expect("missing static method should report a diagnostic");

    assert!(method.message.contains("struct `Gadget`"));
    assert!(static_method.message.contains("type `Gadget`"));
    assert_gust_diagnostic_name(&method.message);
    assert_gust_diagnostic_name(&static_method.message);
    assert_rendered_at(&result, method, &project.path("main.gust"), 5, 5);
    assert_rendered_at(&result, static_method, &project.path("main.gust"), 6, 5);
}

#[test]
fn imported_trait_bound_and_impl_coherence_diagnostics_use_gust_names() {
    let bound_project = TempProject::new();
    bound_project.write(
        "main.gust",
        r#"from ./model import { Named, Person }

fn describe<T: Named>(value: T): String => "description"

fn main() {
    let description = describe(Person {})
}"#,
    );
    bound_project.write(
        "model.gust",
        r#"trait Named {
    fn name(): String
}

struct Person {}"#,
    );

    let bound_result =
        check_project(&bound_project.path("main.gust")).expect("project should load");
    let bound = bound_result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("does not satisfy bound"))
        .expect("unsatisfied bound should report a diagnostic");

    assert!(bound.message.contains("type `Person`"));
    assert!(bound.message.contains("`Person: Named`"));
    assert_gust_diagnostic_name(&bound.message);
    assert_rendered_at(
        &bound_result,
        bound,
        &bound_project.path("main.gust"),
        6,
        32,
    );

    let coherence_project = TempProject::new();
    coherence_project.write(
        "main.gust",
        r#"from ./model import { Describe, Person }

impl Describe for Person {
    fn describe() => "first"
}

impl Describe for Person {
    fn describe() => "second"
}

fn main() {}"#,
    );
    coherence_project.write(
        "model.gust",
        r#"trait Describe {
    fn describe(): String
}

struct Person {}"#,
    );

    let coherence_result =
        check_project(&coherence_project.path("main.gust")).expect("project should load");
    let coherence = coherence_result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("conflicting implementations"))
        .expect("conflicting impls should report a diagnostic");

    assert!(
        coherence
            .message
            .contains("conflicting implementations of trait `Describe` for type `Person`")
    );
    assert_gust_diagnostic_name(&coherence.message);
    assert_rendered_at(
        &coherence_result,
        coherence,
        &coherence_project.path("main.gust"),
        7,
        1,
    );
}

#[test]
fn imported_for_iterable_diagnostics_use_gust_names_in_semantic_and_lowering_phases() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./collection import { ArrayList }

fn main() {
    let values: ArrayList<i32> = ArrayList<i32> {}
    for value in values {}
}"#,
    );
    project.write("collection.gust", "struct ArrayList<T> {}");

    let result = check_project(&project.path("main.gust")).expect("project should load");
    let semantic = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("`for` requires"))
        .expect("invalid iterable should report a semantic diagnostic");

    assert!(semantic.message.contains("got `ArrayList<i32>`"));
    assert_gust_diagnostic_name(&semantic.message);
    assert_rendered_at(&result, semantic, &project.path("main.gust"), 5, 18);

    let lowering = lower_program(&result.program).expect_err("invalid iterable should not lower");
    let lowering = lowering
        .iter()
        .find(|diagnostic| diagnostic.message.contains("`for` requires"))
        .expect("invalid iterable should report a lowering diagnostic");
    assert!(lowering.message.contains("got `ArrayList<i32>`"));
    assert_gust_diagnostic_name(&lowering.message);
    assert_rendered_at(&result, lowering, &project.path("main.gust"), 5, 5);
}

fn assert_gust_diagnostic_name(message: &str) {
    assert!(
        !message.contains("module_"),
        "diagnostic leaked a compiler-internal name: {message}"
    );
    assert!(
        !message.contains("::"),
        "diagnostic used non-Gust qualification syntax: {message}"
    );
}

fn assert_rendered_at(
    result: &gustc::project::ProjectCompileResult,
    diagnostic: &gustc::diagnostic::Diagnostic,
    path: &Path,
    line: usize,
    column: usize,
) {
    let rendered = result.sources.render(diagnostic);
    let path = path
        .canonicalize()
        .expect("diagnostic source path should exist");
    assert!(
        rendered.starts_with(&format!("{}:{line}:{column}:", path.display())),
        "expected diagnostic at {}:{line}:{column}, got {rendered}",
        path.display(),
    );
}

fn path_suffix(path: &str) -> &str {
    Path::new(path).to_str().expect("test path should be UTF-8")
}
