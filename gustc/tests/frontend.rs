use gustc::check_source;
use gustc::diagnostic::Severity;
use gustc::lexer::Lexer;
use gustc::parser::Parser;

#[test]
fn hello_world_has_no_frontend_errors() {
    let source = include_str!("../../examples/hello-world.gust");
    let result = check_source(source);

    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basics_parses_without_syntax_errors() {
    let source = include_str!("../../examples/basics.gust");
    let (tokens, lexer_diagnostics) = Lexer::new(source).tokenize();
    let (_, parser_diagnostics) = Parser::new(tokens).parse();

    assert!(
        lexer_diagnostics.is_empty(),
        "expected no lexer diagnostics, got {lexer_diagnostics:?}"
    );
    assert!(
        parser_diagnostics.is_empty(),
        "expected no parser diagnostics, got {parser_diagnostics:?}"
    );
}

#[test]
fn basics_reports_unsupported_features() {
    let source = include_str!("../../examples/basics.gust");
    let result = check_source(source);

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Warning
                && diagnostic.message.contains("not implemented yet")),
        "expected at least one unsupported-feature warning, got {:?}",
        result.diagnostics
    );
}

#[test]
fn duplicate_top_level_names_are_errors() {
    let result = check_source(
        r#"
fn main() {}
fn main() {}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("duplicate top-level name `main`")),
        "expected duplicate-name error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn missing_main_is_an_error() {
    let result = check_source("fn helper() {}");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("missing `main` function")),
        "expected missing-main error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn unknown_local_reference_is_an_error() {
    let result = check_source(
        r#"
fn main() {
    io.println(name)
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic.message.contains("unknown name `name`")),
        "expected unknown-name error, got {:?}",
        result.diagnostics
    );
}

#[test]
fn immutable_increment_is_an_error() {
    let result = check_source(
        r#"
fn main() {
    let age = 30
    age++
}
"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error
                && diagnostic
                    .message
                    .contains("cannot mutate immutable binding `age`")),
        "expected immutable-mutation error, got {:?}",
        result.diagnostics
    );
}
