#[test]
fn hello_world_has_no_frontend_errors() {
    let source = include_str!("../../../examples/helloWorld.gust");
    let result = check_source(source);

    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got {:?}",
        result.diagnostics
    );
}

#[test]
fn basics_parses_without_syntax_errors() {
    let source = include_str!("../../../examples/milestone.gust");
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
    let source = include_str!("../../../examples/milestone.gust");
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
fn import_aliases_parse_and_define_the_local_name() {
    let source = r#"from package import { External as LocalExternal, helper as localHelper }

fn main() {
    localHelper()
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected aliased imports to validate, got {:?}",
        result.diagnostics
    );

    let Item::Import(import) = &result.program.items[0] else {
        panic!("expected import declaration");
    };
    assert_eq!(import.names[0].name, "External");
    assert_eq!(import.names[0].alias.as_deref(), Some("LocalExternal"));
    assert_eq!(import.names[1].name, "helper");
    assert_eq!(import.names[1].alias.as_deref(), Some("localHelper"));
}

#[test]
fn export_marks_top_level_declarations() {
    let source = r#"export struct External {}

export fn helper(): string => "ok"

fn main() {
    io.println(helper())
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected exported declarations to validate, got {:?}",
        result.diagnostics
    );

    let Item::Struct(struct_) = &result.program.items[0] else {
        panic!("expected struct declaration");
    };
    assert!(struct_.exported);

    let Item::Function(function) = &result.program.items[1] else {
        panic!("expected function declaration");
    };
    assert!(function.exported);
}

#[test]
fn module_namespaces_parse_and_suppress_unknown_member_errors() {
    let source = r#"from package import package

fn main() {
    let value: package.Value = package.Value { name: "Gust" }
    package.print(value)
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected module namespace to validate, got {:?}",
        result.diagnostics
    );

    let Item::Import(import) = &result.program.items[0] else {
        panic!("expected import declaration");
    };
    assert!(import.names.is_empty());
    assert_eq!(
        import
            .namespace
            .as_ref()
            .map(|namespace| namespace.name.as_str()),
        Some("package")
    );
}

#[test]
fn string_interpolation_validates() {
    let source = r#"struct Person {
    name: string
}

fn main() {
    let person = Person { name: "Gust" }
    let count = 2
    let message = "Hello, $person.name ${count + 1}! \$literal"
    io.println(message)
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected string interpolation to validate, got {:?}",
        result.diagnostics
    );
}
