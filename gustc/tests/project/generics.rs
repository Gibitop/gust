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

fn none(): Option<string> => Option.None"#,
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

fn printName(value: Named<string>) {
    io.println(value.name())
}

fn main() {
    let person = Person.new("from module")
    let named: Named<string> = person
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
    name: string

    static fn new(name: string): Self => Self { name: name }
}

impl Named<string> for Person {
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
    let named: Named<string> = value
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
    fn describe(): string
}

struct Person {
    name: string
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
fn imported_generic_extensions_are_monomorphized_after_module_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"from ./extensions import { Box, get, pair }

fn main() {
    let text = Box { value: "Gust" }
    let pairValue = Box<i32>.pair<string>(7, "seven")

    io.println(text.get())
    io.println(pairValue.second)
}"#,
    );
    project.write(
        "extensions.gust",
        r#"struct Box<T> {
    value: T
}

struct Pair<T, U> {
    first: T
    second: U
}

fn Box<T>.get(): T => self.value

static fn Box<T>.pair<U>(value: T, other: U): Pair<T, U> => Pair {
    first: value,
    second: other,
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected imported generic extensions to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("imported generic extensions should lower");
}
