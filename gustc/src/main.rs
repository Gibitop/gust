use std::env;
use std::fs;
use std::process::ExitCode;

use gustc::check_source;
use gustc::span::SourceMap;

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: gustc <file.gust>");
        return ExitCode::FAILURE;
    };

    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{path}: error: failed to read source file: {error}");
            return ExitCode::FAILURE;
        }
    };

    let source_map = SourceMap::new(&source);
    let result = check_source(&source);

    for diagnostic in &result.diagnostics {
        eprintln!("{}", diagnostic.render(&path, &source_map));
    }

    if result.has_errors() {
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
