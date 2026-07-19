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
    let source = r#"export let greeting = "hi"

export struct External {}

export fn helper(): string => "ok"

fn main() {
    io.println(greeting + helper())
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected exported declarations to validate, got {:?}",
        result.diagnostics
    );

    let Item::StaticVar(static_) = &result.program.items[0] else {
        panic!("expected static var declaration");
    };
    assert!(static_.exported);

    let Item::Struct(struct_) = &result.program.items[1] else {
        panic!("expected struct declaration");
    };
    assert!(struct_.exported);

    let Item::Function(function) = &result.program.items[2] else {
        panic!("expected function declaration");
    };
    assert!(function.exported);
}

#[test]
fn top_level_lets_are_immutable_static_bindings() {
    let source = r#"let base = 40
let answer: i32 = base + 2

fn main() {
    io.println(answer.toString())
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected top-level lets to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn single_source_comptime_blocks_expand_before_validation() {
    let source = r#"let answer = comptime {
    return 40 + 2
}

fn main() {
    let value = comptime answer + 1
}"#;
    let result = check_source(source);

    assert!(
        !result.has_errors(),
        "expected single-source comptime expressions to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn block_expression_returns_a_scoped_value() {
    let result = check_source(
        r#"fn answer(): i32 {
    let value = {
        let base = 40
        return base + 2
    }

    return value
}

fn main() {
    let value = answer()
}"#,
    );

    assert!(
        !result.has_errors(),
        "expected block expression to validate, got {:?}",
        result.diagnostics
    );
}

#[test]
fn scoped_block_bindings_do_not_leak() {
    let result = check_source(
        r#"fn main() {
    {
        let a = 42
    }

    io.println(a.toString())
}"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown name `a`")),
        "expected scoped block binding to be unavailable, got {:?}",
        result.diagnostics
    );
}

#[test]
fn block_expression_requires_return_value() {
    let result = check_source(
        r#"fn main() {
    let value = {
        let a = 42
    }
}"#,
    );

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .message
                .contains("block expressions must return a value")),
        "expected missing block return diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn top_level_let_mut_is_rejected() {
    let source = r#"let mut count = 1

fn main() {}"#;
    let result = check_source(source);

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .message
                .contains("top-level static bindings cannot be mutable")),
        "expected top-level let mut diagnostic, got {:?}",
        result.diagnostics
    );
}

#[test]
fn cyclic_top_level_let_initializers_are_rejected() {
    let direct = check_source(
        r#"let a = b + 1
let b = a + 1

fn main() {}"#,
    );

    assert!(
        direct
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .message
                .contains("cyclic top-level let initialization")),
        "expected direct static cycle diagnostic, got {:?}",
        direct.diagnostics
    );

    let through_function = check_source(
        r#"let a = readB()
let b = a + 1

fn readB(): i32 => b

fn main() {}"#,
    );

    assert!(
        through_function
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .message
                .contains("cyclic top-level let initialization")),
        "expected function-hidden static cycle diagnostic, got {:?}",
        through_function.diagnostics
    );

    let through_method = check_source(
        r#"struct Reader {
    static fn read(): i32 => b
}

let a = Reader.read()
let b = a + 1

fn main() {}"#,
    );

    assert!(
        through_method
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .message
                .contains("cyclic top-level let initialization")),
        "expected method-hidden static cycle diagnostic, got {:?}",
        through_method.diagnostics
    );
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
