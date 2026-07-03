use crate::lower::{LoweredProgram, LoweredStatement};

pub fn emit_c(program: &LoweredProgram) -> String {
    let mut source = String::from("#include <stdio.h>\n\nint main(void) {\n");

    for statement in &program.statements {
        match statement {
            LoweredStatement::Println(value) => {
                source.push_str("    puts(\"");

                for byte in value.bytes() {
                    match byte {
                        b'\n' => source.push_str("\\n"),
                        b'\r' => source.push_str("\\r"),
                        b'\t' => source.push_str("\\t"),
                        b'"' => source.push_str("\\\""),
                        b'\\' => source.push_str("\\\\"),
                        b' '..=b'~' => source.push(byte as char),
                        _ => source.push_str(&format!("\\{byte:03o}")),
                    }
                }

                source.push_str("\");\n");
            }
        }
    }

    source.push_str("    return 0;\n}\n");
    source
}
