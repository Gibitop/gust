#[test]
fn indexed_reads_and_writes_resolve_through_standard_traits() {
    let result = check_source(
        r#"trait Index<Key> {
    type Output
    fn index(key: Key): Self.Output
}

trait IndexSet<Key> {
    type Value
    fn indexSet(mut self, key: Key, value: Self.Value): void
}

struct Values {
    value: i32

    fn index(index: usize): string => "member"
}

impl Index<usize> for Values {
    type Output: i32
    fn index(index: usize): i32 => self.value
}

impl IndexSet<usize> for Values {
    type Value: i32
    fn indexSet(mut self, index: usize, value: i32) {
        self.value = value
    }
}

fn main() {
    let mut values = Values { value: 1 }
    let read: i32 = values[0]
    let indexed: Index<usize, type Output: i32> = values
    let dynamicRead: i32 = indexed[0]
    values[0] = 2
}
"#,
    );

    assert!(
        result.diagnostics.is_empty(),
        "expected indexed access to validate through Index and IndexSet, got {:?}",
        result.diagnostics
    );
}

#[test]
fn indexed_writes_require_mutability_and_applicable_impls() {
    let immutable = check_source(
        r#"trait IndexSet<Key> {
    type Value
    fn indexSet(mut self, key: Key, value: Self.Value): void
}

struct Values {}

impl IndexSet<usize> for Values {
    type Value: i32
    fn indexSet(mut self, index: usize, value: i32) {}
}

fn main() {
    let values = Values {}
    values[0] = 1
}
"#,
    );
    assert!(immutable.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot call mutable function `Values.indexSet` through immutable binding `values`")
    }));

    let wrong_key = check_source(
        r#"trait Index<Key> {
    type Output
    fn index(key: Key): Self.Output
}

struct Values {}

impl Index<usize> for Values {
    type Output: i32
    fn index(index: usize): i32 => 1
}

fn main() {
    let values = Values {}
    let value = values["wrong"]
}
"#,
    );
    assert!(wrong_key.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("type `Values` does not implement `Index` for this indexed access")
    }));
}

#[test]
fn compound_indexed_assignment_has_a_dedicated_error() {
    let result = check_source(
        r#"struct Values {}

fn main() {
    let mut values = Values {}
    values[0] += 1
}
"#,
    );

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("compound assignment through indexed access is not supported")
    }));

    let increment = check_source(
        r#"struct Values {}

fn main() {
    let mut values = Values {}
    values[0]++
}
"#,
    );
    assert!(increment.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("increment through indexed access is not supported")
    }));
}
