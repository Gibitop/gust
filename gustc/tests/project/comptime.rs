use gustc::ast::{ExprKind, Item};

#[test]
fn comptime_blocks_expand_before_validation() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"let answer = comptime {
    let base = 40
    return base + 2
}

fn main() {
    let value = comptime answer + 1
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected comptime expressions to validate after expansion, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comptime_reports_runtime_local_references_directly() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"fn main() {
    let a = 1
    let b = comptime {
        return a + 1
    }
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        result.has_errors(),
        "expected runtime-local comptime reference to fail"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("`comptime` expressions cannot read runtime local `a`")),
        "expected direct runtime-local diagnostic, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("failed to compile comptime runner")),
        "expected diagnostic before runner compilation, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comptime_local_shadowing_does_not_report_runtime_local_reference() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"fn main() {
    let a = 1
    let b = comptime {
        let a = 41
        return a + 1
    }
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected comptime-local shadowing to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comptime_rejects_runtime_static_references() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"let a = 1

fn main() {
    let b = comptime a + 1
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        result.has_errors(),
        "expected runtime-static comptime reference to fail"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("`comptime` expressions cannot read runtime static `a`")),
        "expected direct runtime-static diagnostic, got {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("failed to compile comptime runner")),
        "expected diagnostic before runner compilation, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comptime_can_reference_previously_expanded_comptime_static() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"let a = comptime 1

fn main() {
    let b = comptime a + 1
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected comptime-expanded static reference to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comptime_runner_supports_match_methods_generics_and_for_loops() {
    let project = TempProject::new();
    project.write("project.yaml", "");
    project.write(
        "src/main.gust",
        r#"struct Box<T> {
    value: T

    fn get(): T {
        return self.value
    }
}

let matched = comptime {
    let value = 2
    return match value {
        1 => "one",
        2 => "two",
        _ => "other",
    }
}

let methodValue = comptime {
    let box = Box<i32> { value: 41 }
    return box.get() + 1
}

let total = comptime {
    let mut result = 0
    for value in [1, 2, 3] {
        result += value
    }
    return result
}

fn main() {}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected compiled comptime runner to support runtime language features, got {:?}",
        result.diagnostics
    );
    let static_value = |name: &str| {
        result
            .program
            .items
            .iter()
            .find_map(|item| {
                let Item::StaticVar(static_) = item else {
                    return None;
                };
                (static_.name == name).then_some(&static_.value)
            })
            .expect("expanded static should exist")
    };
    assert!(matches!(&static_value("matched").kind, ExprKind::String(value) if value == "two"));
    assert!(matches!(&static_value("methodValue").kind, ExprKind::Number(value) if value == "42"));
    assert!(matches!(&static_value("total").kind, ExprKind::Number(value) if value == "6"));
}

#[test]
fn comptime_runner_stdout_does_not_corrupt_result_artifact() {
    let project = TempProject::new();
    project.write("project.yaml", "");
    project.write(
        "src/main.gust",
        r#"let printed = comptime {
    io.println("debug output")
    return "result"
}

fn main() {}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected stdout during comptime to be separate from the result artifact, got {:?}",
        result.diagnostics
    );
    let value = result
        .program
        .items
        .iter()
        .find_map(|item| {
            let Item::StaticVar(static_) = item else {
                return None;
            };
            (static_.name == "printed").then_some(&static_.value)
        })
        .expect("expanded static should exist");
    assert!(matches!(&value.kind, ExprKind::String(value) if value == "result"));
}

#[test]
fn comptime_runner_panic_becomes_compiler_diagnostic() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"let panicking = comptime {
    panic("boom")
    return 1
}

fn main() {}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        result.has_errors(),
        "expected comptime panic to become a compiler diagnostic"
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("boom")),
        "expected panic message in diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn comptime_runner_materializes_struct_results() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"struct Pair {
    left: i32
    right: string
}

let pair = comptime {
    return Pair { left: 42, right: "answer" }
}

fn main() {}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected struct result to materialize, got {:?}",
        result.diagnostics
    );
    let value = result
        .program
        .items
        .iter()
        .find_map(|item| {
            let Item::StaticVar(static_) = item else {
                return None;
            };
            (static_.name == "pair").then_some(&static_.value)
        })
        .expect("expanded static should exist");
    let ExprKind::StructInit { name, fields, .. } = &value.kind else {
        panic!("expected struct materialization, got {:?}", value.kind);
    };
    assert_eq!(name, "Pair");
    assert!(fields.iter().any(|field| {
        field.name == "left" && matches!(&field.value.kind, ExprKind::Number(value) if value == "42")
    }));
    assert!(fields.iter().any(|field| {
        field.name == "right" && matches!(&field.value.kind, ExprKind::String(value) if value == "answer")
    }));
}

#[test]
fn comptime_runner_materializes_closure_results() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"let callback = comptime {
    return fn(): i32 {
        return 42
    }
}

fn main() {
    let answer = callback()
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected closure result to materialize, got {:?}",
        result.diagnostics
    );
    let value = result
        .program
        .items
        .iter()
        .find_map(|item| {
            let Item::StaticVar(static_) = item else {
                return None;
            };
            (static_.name == "callback").then_some(&static_.value)
        })
        .expect("expanded static should exist");
    assert!(matches!(value.kind, ExprKind::Lambda(_)));
}

#[test]
fn comptime_runner_materializes_mutable_counter_closure() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"let counter = comptime {
    let mut n = 0
    return fn(): i32 {
        n++
        return n
    }
}

fn main() {
    let first = counter()
    let second = counter()
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected mutable closure capture to materialize, got {:?}",
        result.diagnostics
    );
    lower_program(&result.program).expect("materialized counter should lower");
}

#[test]
fn comptime_runner_reports_non_materializable_named_function_results() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"fn answer(): i32 {
    return 42
}

let callback = comptime {
    return answer
}

fn main() {}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(result.has_errors(), "expected named function result to be rejected");
    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("comptime result cannot be materialized as Gust source")),
        "expected non-materializable diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn root_package_can_check_file_permissions_by_default() {
    let project = TempProject::new();
    project.write("project.yaml", "noStd: true\n");
    project.write(
        "src/main.gust",
        r#"let allowed = comptime {
    if comptime.permissions.fs.canRead("./data.txt") {
        return "yes"
    }
    return "no"
}

fn main() {}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected root package file permission check to validate, got {:?}",
        result.diagnostics
    );
    let value = result
        .program
        .items
        .iter()
        .find_map(|item| {
            let Item::StaticVar(static_) = item else {
                return None;
            };
            (static_.name == "allowed").then_some(&static_.value)
        })
        .expect("expanded static should exist");
    assert!(matches!(&value.kind, ExprKind::String(value) if value == "yes"));
}

#[test]
fn dependency_can_check_comptime_file_permissions() {
    let project = TempProject::new();
    project.write(
        "project.yaml",
        r#"noStd: true
dependencies:
  helper: fs:./helper
"#,
    );
    project.write(
        "src/main.gust",
        r#"from helper import { embedded }

fn main() {}"#,
    );
    project.write("helper/project.yaml", "noStd: true\n");
    project.write(
        "helper/src/lib.gust",
        r#"export let embedded = comptime {
    if comptime.permissions.fs.canRead("./secret.txt") {
        return "yes"
    }
    return "no"
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected permission check to avoid denied read, got {:?}",
        result.diagnostics
    );
    let value = result
        .program
        .items
        .iter()
        .find_map(|item| {
            let Item::StaticVar(static_) = item else {
                return None;
            };
            static_
                .name
                .ends_with("embedded")
                .then_some(&static_.value)
        })
        .expect("expanded dependency static should exist");
    assert!(matches!(&value.kind, ExprKind::String(value) if value == "no"));
}

#[test]
fn dependency_can_check_allowlisted_comptime_file_permissions() {
    let project = TempProject::new();
    project.write(
        "project.yaml",
        r#"noStd: true
dependencies:
  helper: fs:./helper
comptimePermissions:
  helper:
    fs:
      - ./schema/**/*.yaml
      - ./embed/**
"#,
    );
    project.write(
        "src/main.gust",
        r#"from helper import { schemaAllowed, secretAllowed }

fn main() {}"#,
    );
    project.write("helper/project.yaml", "noStd: true\n");
    project.write(
        "helper/src/lib.gust",
        r#"export let schemaAllowed = comptime {
    if comptime.permissions.fs.canRead("./schema/public/user.yaml") {
        return "yes"
    }
    return "no"
}

export let secretAllowed = comptime {
    if comptime.permissions.fs.canRead("./secret.txt") {
        return "yes"
    }
    return "no"
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected fs permission checks to validate, got {:?}",
        result.diagnostics
    );
    let static_value = |name: &str| {
        result
            .program
            .items
            .iter()
            .find_map(|item| {
                let Item::StaticVar(static_) = item else {
                    return None;
                };
                static_.name.ends_with(name).then_some(&static_.value)
            })
            .expect("expanded dependency static should exist")
    };
    assert!(
        matches!(&static_value("schemaAllowed").kind, ExprKind::String(value) if value == "yes")
    );
    assert!(
        matches!(&static_value("secretAllowed").kind, ExprKind::String(value) if value == "no")
    );
}

#[test]
fn dependency_can_check_comptime_env_permissions() {
    let project = TempProject::new();
    project.write(
        "project.yaml",
        r#"noStd: true
dependencies:
  helper: fs:./helper
comptimePermissions:
  helper:
    env:
      - PORT
"#,
    );
    project.write(
        "src/main.gust",
        r#"from helper import { portAllowed, secretAllowed }

fn main() {}"#,
    );
    project.write("helper/project.yaml", "noStd: true\n");
    project.write(
        "helper/src/lib.gust",
        r#"export let portAllowed = comptime {
    if comptime.permissions.env.canRead("PORT") {
        return "yes"
    }
    return "no"
}

export let secretAllowed = comptime {
    if comptime.permissions.env.canRead("SECRET_TOKEN") {
        return "yes"
    }
    return "no"
}"#,
    );

    let result = check_project(&project.path("src/main.gust")).expect("project should load");

    assert!(
        !result.has_errors(),
        "expected env permission checks to validate, got {:?}",
        result.diagnostics
    );
    let static_value = |name: &str| {
        result
            .program
            .items
            .iter()
            .find_map(|item| {
                let Item::StaticVar(static_) = item else {
                    return None;
                };
                static_.name.ends_with(name).then_some(&static_.value)
            })
            .expect("expanded dependency static should exist")
    };
    assert!(matches!(&static_value("portAllowed").kind, ExprKind::String(value) if value == "yes"));
    assert!(matches!(&static_value("secretAllowed").kind, ExprKind::String(value) if value == "no"));
}
