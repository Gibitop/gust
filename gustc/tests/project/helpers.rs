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
