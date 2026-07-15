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
        r#"export struct Greeter {
    name: string

    static fn new(name: string): Self => Self { name: name }
}

fn punctuation(): string => "!"

export fn greeting(value: Greeter): string {
    return "Hello, " + value.name + punctuation()
}

export enum Mood {
    Happy
}

export fn mood(): Mood => Mood.Happy"#,
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
fn associated_types_resolve_across_modules() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./producer import { Producer }
from ./counter import { Counter }

fn main() {
    let producer: Producer<type Item: i32> = Counter { value: 7 }
    io.println(producer.next().toString())
}"#,
    );
    project.write(
        "producer.gust",
        r#"export trait Producer {
    type Item
    fn next(): Self.Item
}"#,
    );
    project.write(
        "counter.gust",
        r#"from ./producer import { Producer }

export struct Counter {
    value: i32
}

impl Producer for Counter {
    type Item: i32
    fn next(): i32 => self.value
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected cross-module associated types to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("project should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("Producer_type_Item__i32"));
    assert!(source.contains("gust_method_next"));
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
        r#"export fn fail() {
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
        r#"export fn visible(): string => "visible"
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
fn private_top_level_declarations_are_not_exports() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./helper import { secret }

fn main() {}"#,
    );
    project.write("helper.gust", "fn secret() {}");

    let result = check_project(&project.path("main.gust")).expect("project should load");

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("does not export `secret`")
    }));
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
        r#"export fn broken(): string {
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
        r#"export struct Greeter {
    name: string

    fn label(): string => "member"
}

fn Greeter.label(): string => "extension"
export fn string.withSuffix(suffix: string): string => self + suffix"#,
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
        r#"export fn marker(): string => "marker"
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
        r#"export fn first(): string => "first"
export fn second(): string => "second""#,
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
    project.write("helper.gust", "export fn available() {}");

    let result = check_project(&project.path("main.gust")).expect("project should load");

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("module namespace `helper` does not export `missing`")
    }));
}
