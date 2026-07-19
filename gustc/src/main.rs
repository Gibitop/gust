use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use gustc::build::compile_c_source_to_binary;
use gustc::c_codegen::{CCodegenOptions, emit_c_with_options};
use gustc::lower::lower_program_with_source_files;
use gustc::project::{ProjectOptions, check_project_with_options};

const USAGE: &str = "usage: gustc <file.gust|directory> [options]";
const HELP: &str = "\
usage: gustc <file.gust|directory> [options]

Arguments:
  <file.gust|directory>  Gust source file, or a directory containing main.gust

Options:
  -o <output>            Write the executable to <output>
  --emit-c <output.c>    Write generated C source to <output.c>
  --std-path <path>      Use the standard library project at <path>
  --gc-stress            Emit a binary that collects at every safepoint
  --help                 Print this help message";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };

    if path == "--help" {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }

    let requested_path = PathBuf::from(&path);
    let source_path = if requested_path.is_dir() {
        if requested_path.join("project.yaml").is_file() {
            requested_path.clone()
        } else {
            requested_path.join("main.gust")
        }
    } else {
        requested_path.clone()
    };
    let mut output_path = None;
    let mut emit_c_path = None;
    let mut std_path = None;
    let mut gc_stress = false;

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
            "--std-path" => {
                let Some(path) = args.next() else {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                };

                if std_path.replace(PathBuf::from(path)).is_some() {
                    eprintln!("duplicate `--std-path` argument");
                    return ExitCode::FAILURE;
                }
            }
            "--gc-stress" => {
                if gc_stress {
                    eprintln!("duplicate `--gc-stress` argument");
                    return ExitCode::FAILURE;
                }
                gc_stress = true;
            }
            "--help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("unexpected argument `{arg}`");
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }

    let output_path = output_path.unwrap_or_else(|| {
        if source_path.is_dir() {
            return source_path.join(
                source_path
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("main")),
            );
        }

        if let Some(stem) = source_path.file_stem() {
            source_path.with_file_name(stem)
        } else {
            source_path.with_extension("out")
        }
    });

    let result = match check_project_with_options(
        &source_path,
        ProjectOptions {
            std_path,
            no_std: false,
        },
    ) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("{path}: error: {error}");
            return ExitCode::FAILURE;
        }
    };

    for diagnostic in &result.diagnostics {
        eprintln!("{}", result.sources.render(diagnostic));
    }

    if result.has_errors() {
        return ExitCode::FAILURE;
    }

    let lowered = match lower_program_with_source_files(
        &result.program,
        result.sources.to_lowering_source_files(),
    ) {
        Ok(program) => program,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!("{}", result.sources.render(&diagnostic));
            }

            return ExitCode::FAILURE;
        }
    };

    let c_source = emit_c_with_options(&lowered, CCodegenOptions { gc_stress });
    let keep_c_file = emit_c_path.is_some();
    let c_path = emit_c_path.unwrap_or_else(|| {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());

        env::temp_dir().join(format!("gustc-{}-{unique_id}.c", process::id()))
    });

    let output = compile_c_source_to_binary(&c_source, &c_path, &output_path);

    if !keep_c_file {
        let _ = fs::remove_file(&c_path);
    }

    match output {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
