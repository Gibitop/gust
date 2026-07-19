use std::fs;
use std::path::Path;
use std::process::Command;

pub fn compile_c_source_to_binary(
    c_source: &str,
    c_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    fs::write(c_path, c_source).map_err(|error| {
        format!(
            "{}: error: failed to write generated C source: {error}",
            c_path.display()
        )
    })?;

    let output = Command::new("cc")
        .arg(c_path)
        .arg("-lm")
        .arg("-o")
        .arg(output_path)
        .output()
        .map_err(|error| format!("error: failed to invoke cc: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let mut message = String::new();
    if !output.stdout.is_empty() {
        message.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        message.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    message.push_str(&format!("cc failed with status {}", output.status));
    Err(message)
}
