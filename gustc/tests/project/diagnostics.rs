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
        r#"export struct ArrayList<T> {
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
    project.write("model.gust", "export struct Gadget {}");

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

fn describe<T: Named>(value: T): string => "description"

fn main() {
    let description = describe(Person {})
}"#,
    );
    bound_project.write(
        "model.gust",
        r#"export trait Named {
    fn name(): string
}

export struct Person {}"#,
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
        r#"export trait Describe {
    fn describe(): string
}

export struct Person {}"#,
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
    project.write("collection.gust", "export struct ArrayList<T> {}");

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
