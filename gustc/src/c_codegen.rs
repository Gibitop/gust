use crate::lower::{LoweredProgram, LoweredStatement, LoweredValue};

pub fn emit_c(program: &LoweredProgram) -> String {
    let mut source = String::from("#include <stdio.h>\n\nint main(void) {\n");

    for statement in &program.statements {
        match statement {
            LoweredStatement::StringLocal { name, value } => {
                source.push_str("    const char* ");
                source.push_str(name);
                source.push_str(" = \"");
                push_c_string_value(&mut source, value);
                source.push_str("\";\n");
            }
            LoweredStatement::Println(value) => match value {
                LoweredValue::StringLiteral(value) => {
                    source.push_str("    puts(\"");
                    push_c_string_value(&mut source, value);
                    source.push_str("\");\n");
                }
                LoweredValue::StringLocal(name) => {
                    source.push_str("    puts(");
                    source.push_str(name);
                    source.push_str(");\n");
                }
            },
        }
    }

    source.push_str("    return 0;\n}\n");
    source
}

fn push_c_string_value(source: &mut String, value: &str) {
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
}
