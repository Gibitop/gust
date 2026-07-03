use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use gustc::c_codegen::emit_c;
use gustc::check_source;
use gustc::lower::lower_program;
use gustc::span::SourceMap;

const USAGE: &str = "usage: gustc <file.gust> [-o <output>] [--emit-c <output.c>]";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };

    let source_path = PathBuf::from(&path);
    let mut output_path = None;
    let mut emit_c_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" => {
                let Some(output) = args.next() else {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                };

                if output_path.replace(PathBuf::from(output)).is_some() {
                    eprintln!("duplicate `-o` argument");
                    return ExitCode::FAILURE;
                }
            }
            "--emit-c" => {
                let Some(output) = args.next() else {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                };

                if emit_c_path.replace(PathBuf::from(output)).is_some() {
                    eprintln!("duplicate `--emit-c` argument");
                    return ExitCode::FAILURE;
                }
            }
            _ => {
                eprintln!("unexpected argument `{arg}`");
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }

    let output_path = output_path.unwrap_or_else(|| {
        if let Some(stem) = source_path.file_stem() {
            source_path.with_file_name(stem)
        } else {
            source_path.with_extension("out")
        }
    });

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

    let lowered = match lower_program(&result.program) {
        Ok(program) => program,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!("{}", diagnostic.render(&path, &source_map));
            }

            return ExitCode::FAILURE;
        }
    };

    let c_source = emit_c(&lowered);
    let keep_c_file = emit_c_path.is_some();
    let c_path = emit_c_path.unwrap_or_else(|| {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());

        env::temp_dir().join(format!("gustc-{}-{unique_id}.c", process::id()))
    });

    if let Err(error) = fs::write(&c_path, &c_source) {
        eprintln!(
            "{}: error: failed to write generated C source: {error}",
            c_path.display()
        );
        return ExitCode::FAILURE;
    }

    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&output_path)
        .output();

    if !keep_c_file {
        let _ = fs::remove_file(&c_path);
    }

    match output {
        Ok(output) if output.status.success() => ExitCode::SUCCESS,
        Ok(output) => {
            if !output.stdout.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&output.stdout));
            }

            if !output.stderr.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
            }

            eprintln!("cc failed with status {}", output.status);
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: failed to invoke cc: {error}");
            ExitCode::FAILURE
        }
    }
}
