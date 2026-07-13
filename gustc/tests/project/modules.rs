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
    name: string

    static fn new(name: string): Self => Self { name: name }
}

fn punctuation(): string => "!"

fn greeting(value: Greeter): string {
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
fn panic_stack_locations_use_paths_relative_to_compilation_root() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./lib/helper import { fail }

fn main() {
    fail()
}"#,
    );
    project.write(
        "lib/helper.gust",
        r#"fn fail() {
    panic("from helper")
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected project to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program_with_source_files(
        &result.program,
        result.sources.to_lowering_source_files(),
    )
    .expect("project should lower");
    let source = emit_c(&lowered);

    assert!(source.contains("gust_rt_stack_push(\"main\", \"main.gust\", 3, 1);"));
    assert!(source.contains("\", \"lib/helper.gust\", 1, 1);"));
    assert!(source.contains("gust_rt_stack_update(\"lib/helper.gust\", 2, 5);"));
    assert!(
        !source.contains(&project.path.to_string_lossy().into_owned()),
        "generated panic frames should not contain absolute temp project paths: {source}"
    );
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
        r#"fn visible(): string => "visible"
fn hidden(): string => "hidden""#,
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
        r#"fn broken(): string {
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
                .contains("expected value of type `string`")
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

fn value(): string => other()"#,
    );
    project.write(
        "b.gust",
        r#"from ./a import { value }

fn other(): string => value()"#,
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
    name: string

    fn label(): string => "member"
}

fn Greeter.label(): string => "extension"
fn string.withSuffix(suffix: string): string => self + suffix"#,
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
        r#"fn marker(): string => "marker"
fn string.withSuffix(suffix: string): string => self + suffix"#,
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
        r#"fn first(): string => "first"
fn second(): string => "second""#,
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
