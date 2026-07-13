#[test]
fn root_standard_library_modules_link_through_relative_imports() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../../std/option.gust"));
    project.write("std/iter.gust", include_str!("../../../std/iter.gust"));
    project.write(
        "examples/main.gust",
        r#"from ../std/iter import { Iterator }
from ../std/option import { Option }

struct Counter {
    value: i32
}

impl Iterator for Counter {
    type Item: i32
    fn next(mut self): Option<i32> {
        let value = self.value
        self.value++
        return Option.Some(value)
    }
}

fn main() {
    let mut iterator: Iterator<type Item: i32> = Counter { value: 1 }
    let message = match iterator.next() {
        Option.Some(value) => value.toString(),
        Option.None => "empty",
    }
    io.println(message)
}"#,
    );

    let result = check_project(&project.path("examples/main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected root standard library modules to validate, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("root standard library modules should lower");
}

#[test]
fn result_methods_link_and_run() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../../std/option.gust"));
    project.write("std/result.gust", include_str!("../../../std/result.gust"));
    project.write(
        "main.gust",
        r#"from ./std/option import { Option }
from ./std/result import { Result }

fn main() {
    let success: Result<i32, string> = Result.Ok(42)
    let failure: Result<i32, string> = Result.Err("failed")

    if success.isOk() {
        io.println("true")
    }
    if failure.isErr() {
        io.println("true")
    }
    io.println(success.unwrap().toString())
    io.println(failure.unwrapOr(7).toString())
    io.println(failure.err().unwrapOr("missing"))
    io.println(success.ok().unwrap().toString())
    io.println(failure.unwrapErr())
    io.println(success.expect("unexpected error").toString())
    io.println(failure.expectErr("unexpected success"))
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected Result project to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("Result project should lower");
    let c_path = project.path("result.c");
    fs::write(&c_path, emit_c(&lowered)).expect("generated C should be written");
    let executable = project.path("result");
    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build Result executable");
    assert!(
        output.status.success(),
        "generated Result C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable)
        .output()
        .expect("Result executable should run");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "true\ntrue\n42\n7\nfailed\n42\nfailed\n42\nfailed\n"
    );
}

#[test]
fn result_expect_panics_with_the_provided_message() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../../std/option.gust"));
    project.write("std/result.gust", include_str!("../../../std/result.gust"));
    project.write(
        "main.gust",
        r#"from ./std/result import { Result }

fn main() {
    let failure: Result<i32, string> = Result.Err("failed")
    failure.expect("result was required")
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected Result panic project to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("Result panic project should lower");
    let c_path = project.path("result-panic.c");
    fs::write(&c_path, emit_c(&lowered)).expect("generated C should be written");
    let executable = project.path("result-panic");
    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build Result panic executable");
    assert!(
        output.status.success(),
        "generated Result panic C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable)
        .output()
        .expect("Result panic executable should run");
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("panic: result was required"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn string_intrinsics_are_available_without_imports() {
    let project = TempProject::new();
    project.write(
        "examples/main.gust",
        r#"fn main() {
    let value = "Gust"
    if value.byteLen() == 4 && !value.isEmpty() {
        io.println(value)
    }
}"#,
    );

    let result = check_project(&project.path("examples/main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected intrinsic string operations to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("intrinsic string operations should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("gust_value.gust_byte_len"), "{source}");
    assert!(source.contains("gust_value.gust_byte_len == 0"), "{source}");
}

#[test]
fn string_builder_uses_growable_runtime_storage() {
    let project = TempProject::new();
    project.write(
        "std/internal/stringBuilder.gust",
        include_str!("../../../std/internal/stringBuilder.gust"),
    );
    project.write(
        "examples/main.gust",
        r#"from ../std/internal/stringBuilder import { StringBuilder }

fn main() {
    let mut builder = StringBuilder.withCapacity(1)
    builder.append("hello")
    builder.append(" world")
    io.println(builder.build())
}"#,
    );

    let result = check_project(&project.path("examples/main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected StringBuilder to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("StringBuilder should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("gust_rt_string_builder_append_"));
    assert!(source.contains("gust_rt_string_builder_build_"));
    assert!(!source.contains("gust_marker"));
}

#[test]
fn std_internal_declares_compiler_backed_storage_types() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../../std/option.gust"));
    project.write(
        "std/internal/rawBuffer.gust",
        include_str!("../../../std/internal/rawBuffer.gust"),
    );
    project.write(
        "std/internal/stringBuilder.gust",
        include_str!("../../../std/internal/stringBuilder.gust"),
    );
    project.write(
        "main.gust",
        r#"from ./std/internal/rawBuffer import { RawBuffer }
from ./std/internal/stringBuilder import { StringBuilder }

fn main() {
    let value = "gust"
    io.println(value.byteLen().toString())
    io.println(value.len().toString())
    if value.isEmpty() {
        io.println("empty")
    } else {
        io.println("not empty")
    }

    let mut builder = StringBuilder.new()
    builder.append(value)
    io.println(builder.build())

    let mut buffer = RawBuffer<i32>.withCapacity(1)
    buffer.write(0, 42)
    io.println(buffer.capacity().toString())
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected std/internal storage declarations to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("std/internal storage should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("RawBuffer<i32>"));
    assert!(source.contains("sizeof(int32_t) * gust_buffer->gust_capacity"));
    assert!(!source.contains("gust_empty"));
}

#[test]
fn collection_literals_lower_through_from_elements() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../../std/option.gust"));
    project.write("std/result.gust", include_str!("../../../std/result.gust"));
    project.write("std/iter.gust", include_str!("../../../std/iter.gust"));
    project.write("std/index.gust", include_str!("../../../std/index.gust"));
    project.write(
        "std/collection.gust",
        include_str!("../../../std/collection.gust"),
    );
    project.write(
        "std/internal/rawBuffer.gust",
        include_str!("../../../std/internal/rawBuffer.gust"),
    );
    project.write(
        "std/arrayList.gust",
        include_str!("../../../std/arrayList.gust"),
    );
    project.write(
        "main.gust",
        r#"from ./std/arrayList import { ArrayList }
from ./std/collection import { FromElements }

struct TestCollection<T> {
    values: ArrayList<T>
}

impl<T> FromElements for TestCollection<T> {
    type Item: T
    static fn withElementCapacity(capacity: usize): Self => TestCollection<T> {
        values: ArrayList<T>.withCapacity(capacity),
    }

    fn add(mut self, value: T): void {
        self.values.push(value)
    }
}

fn main() {
    let mut values = [1, 2, 3]
    let indexed = values[0]
    let indexReplaced = values[0]
    values[0] = 10
    values.push(4)
    let replaced = values.set(1, 20)
    let rejected = values.set(10, 100)
    let popped = values.pop()
    let custom: TestCollection<i32> = [5, 6]
    io.println(values.len().toString())
    io.println(indexed.toString())
    io.println(indexReplaced.toString())
    io.println(replaced.unwrapOr(-1).toString())
    io.println(rejected.err().unwrapOr("missing"))
    let iterator = values.iterator()
    let copied = ArrayList.fromIterator(iterator)
    io.println(copied.len().toString())

    for value in copied {
        io.println(value.toString())
    }
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected collection project to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("collection project should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("gust_collection"));
    assert!(source.contains("gust_data"));
    let c_path = project.path("collection.c");
    fs::write(&c_path, source).expect("generated C should be written");
    let output = Command::new("cc")
        .arg("-fsyntax-only")
        .arg(&c_path)
        .output()
        .expect("C compiler should run");
    assert!(
        output.status.success(),
        "generated collection C should compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let executable = project.path("collection");
    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build collection executable");
    assert!(
        output.status.success(),
        "generated collection C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable)
        .output()
        .expect("collection executable should run");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "3\n1\n1\n2\nindex out of bounds\n3\n10\n20\n3\n"
    );
}

#[test]
fn array_list_indexed_reads_and_writes_panic_out_of_bounds() {
    assert_array_list_index_panic(
        r#"from ./std/arrayList import { ArrayList }

fn main() {
    let values = [1]
    io.println(values[1].toString())
}
"#,
        "read",
    );
    assert_array_list_index_panic(
        r#"from ./std/arrayList import { ArrayList }

fn main() {
    let mut values = [1]
    values[1] = 2
}
"#,
        "write",
    );
}

fn assert_array_list_index_panic(main: &str, name: &str) {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../../std/option.gust"));
    project.write("std/result.gust", include_str!("../../../std/result.gust"));
    project.write("std/iter.gust", include_str!("../../../std/iter.gust"));
    project.write("std/index.gust", include_str!("../../../std/index.gust"));
    project.write(
        "std/collection.gust",
        include_str!("../../../std/collection.gust"),
    );
    project.write(
        "std/internal/rawBuffer.gust",
        include_str!("../../../std/internal/rawBuffer.gust"),
    );
    project.write(
        "std/arrayList.gust",
        include_str!("../../../std/arrayList.gust"),
    );
    project.write("main.gust", main);

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected indexed {name} panic project to validate, got {:?}",
        result.diagnostics
    );
    let lowered = lower_program(&result.program).expect("indexed panic project should lower");
    let c_path = project.path(&format!("index-{name}.c"));
    fs::write(&c_path, emit_c(&lowered)).expect("generated C should be written");
    let executable = project.path(&format!("index-{name}"));
    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build indexed panic executable");
    assert!(
        output.status.success(),
        "generated indexed panic C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable)
        .output()
        .expect("indexed panic executable should run");
    assert_eq!(output.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&output.stderr).contains("panic: index out of bounds"));
}

#[test]
fn range_literals_iterate_with_project_modules() {
    let project = TempProject::new();
    project.write("std/option.gust", include_str!("../../../std/option.gust"));
    project.write("std/iter.gust", include_str!("../../../std/iter.gust"));
    project.write("std/range.gust", include_str!("../../../std/range.gust"));
    project.write(
        "main.gust",
        r#"from ./std/range import { Range, RangeInclusive }

fn label(value: i32): string {
    return match value {
        0 => "zero",
        1..4 => "small",
        4..=6 => "medium",
        _ => "other",
    }
}

fn main() {
    for value in 1..4 {
        io.println(label(value))
    }

    for value in 4..=6 {
        io.println(label(value))
    }
}"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected range project to validate, got {:?}",
        result.diagnostics
    );

    let lowered = lower_program(&result.program).expect("range project should lower");
    let source = emit_c(&lowered);
    assert!(source.contains("Range"));
    assert!(source.contains("RangeInclusive"));
    let c_path = project.path("range.c");
    fs::write(&c_path, source).expect("generated C should be written");
    let executable = project.path("range");
    let output = Command::new("cc")
        .arg(&c_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C compiler should build range executable");
    assert!(
        output.status.success(),
        "generated range C should build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable)
        .output()
        .expect("range executable should run");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "small\nsmall\nsmall\nmedium\nmedium\nmedium\n"
    );
}

#[test]
fn enum_methods_survive_project_linking() {
    let project = TempProject::new();
    project.write(
        "main.gust",
        r#"enum Option<T> {
    Some(T)
    None

    fn unwrapOr(fallback: T): T {
        return match self {
            Option.Some(value) => value,
            Option.None => fallback,
        }
    }
}

fn main() {
    let present = Option.Some(42)
    io.println(present.unwrapOr(0).toString())
}
"#,
    );

    let result = check_project(&project.path("main.gust")).expect("project should load");
    assert!(
        result.diagnostics.is_empty(),
        "expected enum methods to validate after linking, got {:?}",
        result.diagnostics
    );

    lower_program(&result.program).expect("linked enum methods should lower");
}
