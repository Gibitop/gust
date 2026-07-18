# Development decisions log

## First toolchain

First toolchain will be implemented in the rust programming language. Later the toolchain will be ported to the gust language itself

## Gust project structure

Minimal gust project contains a single `.gust` file with a `main` function

A full Gust project contains `project.yaml` and a `src` directory. `src/main.gust` is the
executable entry point used when compiling a project directory. It must define the program's
`main` function, and it is not special when the project is used only as a dependency.

`src/lib.gust` is the package root module used by bare dependency imports. If another project
declares `helper: fs:../helper`, then `from helper import { value }` loads
`helper/src/lib.gust`. A dependency may also expose submodules directly:
`from helper/path import { value }` loads `helper/src/path.gust`. A project can contain both
`main.gust` and `lib.gust` when it is both executable and importable. Library-only projects do not
need `main.gust`; executable-only projects do not need `lib.gust` unless consumers import the
package root.

Compiling a `.gust` file still works without a full project; if the file is inside a directory tree
with `project.yaml`, dependency resolution uses that project as the package root.

`project.yaml` may declare package dependencies:

```yaml
dependencies:
  helper: fs:../helper
```

For now, only `fs:` dependencies are supported. The path may be absolute or relative to the
declaring project's root, and it must point to another full Gust project. A package import resolves
through the importing package's own dependency table.
Dependencies keep their own dependency scopes, so an application and one of its dependencies may
resolve the same dependency name to different packages. Internal module names include the package
root and source-relative module path so package instances do not collide.
Relative imports inside full projects must stay under that package's `src` directory.

The dependency name `std` is reserved for the compiler-owned standard library and cannot be
declared in `project.yaml`.

`project.yaml` may set `noStd: true` to make the standard library unavailable to that package.
`noStd` packages do not receive the implicit standard prelude and cannot import `std/...`. This is
used by the standard-library project itself while it is being compiled from source.

## Modules

Local modules use relative paths and named imports. A relative import without an extension resolves
to a `.gust` file next to the importing module. Only top-level declarations marked with `export`
are available for named import or through a module namespace. Imported names can be bound to a
different local name with `from ./module import { original as localName }`.

An unbraced import binds the module as a namespace:
`from ./module import namespace`. Declarations are then accessed through the namespace, such as
`namespace.function()` or `namespace.Struct { value: 1 }`. Extension functions must still be
imported by name so their availability remains explicit at method call sites.

The compiler loads the complete local module graph before semantic analysis. Imported modules use
deterministic internal names derived from their import path so declarations with the same source
name in different modules do not collide. Only names listed by an importing module are added to
that module's scope. Extension functions follow the same rule and retain real-member precedence.

`from ./module import *` imports all exported names from the target module into the current module.
Explicit star imports use normal import conflict rules: a star-imported name that conflicts with a
local declaration, namespace import, or another imported name is an error.

Modules can re-export names from another module with `from ./module export { Name }`, and can
re-export every exported name with `from ./module export *`. Re-exports do not bind the names for
local use in the re-exporting module.

Package module resolution supports direct `fs:` project dependencies. Import cycles are rejected.

Unexported top-level declarations remain visible inside their declaring module and are still linked
when an exported declaration uses them, but other modules cannot import them by name or access them
through a namespace import.

## Standard library development

The source-level standard library lives in the repository-root `std` project, with source files
under `std/src`. The compiler treats `std` as a compiler-owned package, so Gust projects can import
modules such as `from std/iter import { Iterator }` without declaring `std` in `project.yaml`.
Non-project examples may still import standard-library modules through ordinary relative paths,
such as `from ../std/src/iter import { Iterator }`.
Import paths use `/` as their separator. Standard-library modules may import one another using
relative paths. Gust source filenames use camelCase.

`gustc --std-path <path>` points the compiler at the standard-library project to use. Without the
flag, the CLI looks for `../std` relative to the compiler executable, with a repository-local
fallback for development builds.

For every package that does not set `noStd: true`, the compiler adds a weak implicit star import
from `std/prelude`. Weak prelude names are used only when the module does not already define or
explicitly import that name, so adding names to the standard prelude does not break existing local
declarations. Dependencies receive the prelude according to their own package configuration, not
according to the root package that caused them to be compiled.

Fully compiler-owned primitive types, such as numeric types, `char`, and `string`, do not have source-level
standard-library declarations for their intrinsic members. This keeps operations that are always
available, such as numeric `toString()` and `string.len()`, from looking like extension functions
that users need to import.

## Collection literals

`[value, ...]` is a collection literal. When a surrounding type supplies a concrete collection,
that collection must implement `FromElements<type Item: T>`; without a target type, the literal defaults to
an imported `ArrayList<T>`. The compiler lowers the literal to the collection's
`withElementCapacity` and `add` trait-implementation functions, preserving left-to-right element
evaluation. Collection behaviour therefore remains standard-library code.

`FromIterator<type Item: T>` is a separate standard-library construction trait for iterators. `ArrayList<T>`
implements both traits. The internal `RawBuffer<T>` storage type is the only collection-specific
runtime primitive; its compiler-implemented declaration lives in `std/src/internal/rawBuffer.gust`.
Its allocation and typed storage operations are lowered by the executable backend so future GC
integration stays behind that boundary.

## Iterator adapters

`map`, `filter`, and `collect` are lazy provided methods of `Iterator`, so they are available on
every iterator without importing extension functions. They return a trait-typed iterator backed by
standard-library adapter structs, so calls can be chained without compiler-owned iterator syntax.
`map` accepts `fn(T): U`; `filter` accepts `fn(T): bool`; and `collect` delegates to
`FromIterator<type Item: T>.fromIterator`.

Trait instance methods may have a body and declare their own type parameters. Implementations need
only provide bodyless trait methods. The compiler specializes provided methods for the concrete trait
arguments and associated-type bindings at a call site, then dispatches them statically; they are not
vtable entries. `Iterator.next` remains dynamically dispatched through the iterator value.
The adapter structs are named `MapIterator` and `FilterIterator` to distinguish them from
map-like collections and filter values.

`FromIterator` is declared alongside `Iterator` in `std/src/iter.gust`, avoiding an import cycle while
allowing `collect` to be a provided iterator method. `FromElements` remains in
`std/src/collection.gust`.

## Indexed access

`value[key]` is indexed read syntax. The compiler resolves it through the standard-library
`Index<Key>` trait's `index` method, so its type is `Index.Output`. `value[key] = newValue`
resolves through `IndexSet<Key>.indexSet`, accepts `IndexSet.Value`, and requires a mutable-capable
receiver. Indexed syntax uses qualified trait resolution: real members and extensions named `index`
or `indexSet` do not override the syntax-defined operation.

`ArrayList` treats bracket access as an assertion that the index is valid. An out-of-bounds bracket
read or write panics with `index out of bounds`. Callers that expect an index to be absent use
`get(index): Option<T>` or `set(index, value): Result<T, string>` instead. The traits do not impose
that policy on every collection: an associative collection may choose `Option<V>` as its
`Index.Output` while accepting `V` as its separate `IndexSet.Value`.

Compound indexed assignments and indexed increments are not supported by the initial lowering.
Indexing remains standard-library behaviour: the compiler owns the bracket syntax and trait dispatch
but does not hard-code `ArrayList` storage, bounds policy, or key semantics.

## Standard collection types

The standard library provides `Deque<T>`, `HashMap<K, V>`, `HashSet<T>`, `BinaryHeap<T>`,
`OrderedMap<K, V>`, `OrderedSet<T>`, and `LinkedList<T>` as source-level collection
implementations. `Deque`, `BinaryHeap`, `LinkedList`, and the set types implement
`FromElements` and `FromIterator`; map types use `MapEntry<K, V>` as their iterator and
construction item. Associative map indexing returns `Option<V>` rather than panicking on absent
keys.

Generic equality and ordering use associated-type traits: `Eq<type Other: T>` provides
`equals(other: T)`, and `Ord<type Other: T>` provides `compare(other: T): Ordering`. `Hash`
provides `hash(): u64`. Primitive impls live in the standard library for bool, char, string, and
integer types where the operation is supported. Floating-point types intentionally do not implement
these first equality/hash/order traits because `NaN` and total ordering policy should be decided
separately.

`HashMap` is an open-addressed table implemented with standard-library code over `RawBuffer`.
It stores bucket state in a `RawBuffer<u8>` plus parallel key and value buffers so sparse bucket
state remains explicit and does not depend on `RawBuffer` tracking initialized holes.

Generated C emits prototypes for all lowered functions before function bodies. This supports
mutually recursive methods created by standard-library generics, such as `HashMap.insert` calling
`ensureCapacity` while rehashing calls back into `insert`.

## Option and Result

`Option<T>` represents expected absence, including collection lookup misses, empty collection
removal, and iterator exhaustion. `Result<T, E>` represents operations that can either succeed or
fail with a recoverable error. Standard-library methods panic only when their contract has been
violated or when the caller explicitly requests extraction with `unwrap`, `unwrapErr`, `expect`,
or `expectErr`.

## Syntax

Syntax is very similar to Rust. Notable differences:
- No semicolon
- No implicit return keyword
- All `::` are replaced with `.`
- Function return types are defined with `:` instead of `->`
- Function return types are optional and can be inferred by the compiler
- We prefer camelCase where rust uses snake_case
- No syntax for features missing by design or not implemented in this language yet (eg. life times, macros, etc)
- Imports are done very differently from rust. See the examples/modules directory for an example

## string memory management

Gust will use garbage collection for managed values, including strings. Do not introduce ownership or lexical `free` semantics for strings as an interim design.

Strings are immutable valid UTF-8 text. The runtime representation stores a byte pointer and byte
length rather than relying on a NUL-terminated C string, so embedded NUL bytes are preserved and
runtime operations are bounded by explicit lengths. The byte representation remains opaque to Gust
programs. Fundamental string operations are intrinsic members; higher-level operations belong in
the standard library as the necessary string and Unicode primitives become available.

`StringBuilder` is a standard-library mutable construction type. The compiler supplies only its
opaque growable UTF-8 byte storage and the bridge from `build()` to immutable `string`; its
compiler-implemented declaration and API live in `std/src/internal/stringBuilder.gust`.

The current C backend may temporarily leak heap-allocated string concat results. Keep allocation isolated behind Gust-shaped runtime helpers, so raw `malloc` usage can later be replaced by GC allocation.

## String interpolation

String interpolation follows Kotlin-style syntax. `$name` interpolates a simple identifier, and
`$value.member` may continue through member access. `${expression}` interpolates an arbitrary Gust
expression and is also used to disambiguate identifier boundaries. `\$` writes a literal dollar sign.

The parser lowers interpolation to ordinary string concatenation. Interpolated values are converted
through zero-argument `toString()` calls, with `string.toString()` treated as an intrinsic identity.

## Runtime development

Generated C should route operations that will later be runtime-managed through Gust-shaped helpers instead of calling C primitives directly.

Managed allocations in executable C builds use descriptor-aware `gust_rt_alloc(desc, size)` calls.
The emitted runtime keeps a non-moving heap object header, type descriptors, trace callbacks,
mark/sweep helpers, a safepoint hook, precise generated root slots for parameters and locals, and
a no-op pointer write-barrier hook. Safepoints mark registered roots and sweep unreachable heap
objects when the allocation threshold is reached. The compiler `--gc-stress` flag emits a binary
that forces collection at every safepoint for generated-C tests. Future concurrent or generational
collectors should preserve the
compiler/runtime contract: generated code allocates with descriptors, emitted descriptors trace
managed references, pointer writes go through the runtime barrier abstraction where needed, and
safepoints are the coordination boundary.

## Panics

`panic(message)` is a compiler intrinsic that accepts a single `string` value. Executable builds
lower it to a Gust runtime helper that prints `panic: <message>` to stderr, prints a Gust call
stack from the current frame outward with source `path:line:column` locations relative to the
compilation root, and exits the process with status code 101. For now, the compilation root is the directory
containing the entry source file; later it should become the Gust project root. Generated user
functions and `main` maintain that stack with `gust_rt_stack_push` and `gust_rt_stack_pop` only when
a program uses `panic`, so existing generated C stays unchanged for non-panicking programs. A panic
statement updates the current frame location to the `panic(...)` source line before printing.
Function call emission updates the caller frame to the call expression location before entering the
callee, so caller stack frames point at call sites rather than function definitions.

`panic(...)` is a terminating statement for return-path validation. An exhaustive match whose every
branch returns a value or panics also satisfies a function's return requirement.

## Generated C naming

Generated C reserves `gust_rt_*` for runtime helpers. User-defined symbols must not use this prefix.

User-defined functions should use deterministic internal names shaped like `gust_fn_<hash>_<source_name>` with a nearby comment containing the original Gust function name.

User-defined structs should use deterministic internal names shaped like `gust_struct_<hash>_<source_name>` with a nearby comment containing the original Gust struct name.

User-defined enums should use deterministic internal names shaped like `gust_enum_<hash>_<source_name>` with a nearby comment containing the original Gust enum name. The C backend represents enums as a tag plus a payload union.

Generated local variables and struct fields should use `gust_<source_name>`. Keep source-name suffixes sanitized so generated identifiers stay valid C identifiers.


## Equality

`==` and `!=` compare numeric and boolean values directly. Strings compare by value through a Gust runtime helper, never by backend pointer identity.

Struct and enum equality will be introduced with trait-based equality rather than receiving implicit field-by-field semantics.


## Logical operators

`!`, `&&`, and `||` operate only on boolean values.

`&&` and `||` evaluate left to right and short-circuit the right operand.

When used as an expression statement, `condition && action()` and `condition || action()` are
conditional execution forms. The left operand must be boolean, while the right operand may be any
valid expression statement, including a function that returns `void`. The `&&` form executes the
right operand when the condition is true; the `||` form executes it when the condition is false.
Logical operators used as values still require boolean operands on both sides.

## While loops

`while` conditions must be boolean. A `while` body has block scope, so bindings declared inside
the body do not escape. `break` and `continue` are statements and may only be used inside loop
bodies. Executable builds lower `while`, `break`, and `continue` directly to C control flow.

## Iterable for loops

`for value in iterable` accepts a value that implements either `Iterator<type Item: T>` or
`Iterable<type Item: T>`.
An iterator is used directly; an iterable first produces one through `iterator()`. The compiler
evaluates the iterable expression once, keeps the resulting iterator in a hidden mutable binding,
and repeatedly matches `next()` against `Option.Some(value)` and `Option.None`. Loop bindings are
immutable and scoped to the loop body. Iterating an `Iterator<type Item: T>` directly requires a
mutable-capable expression because `next()` advances it; an `Iterable<type Item: T>` may be iterated through
an immutable binding because it produces the iterator. `break` and `continue` apply to the
generated loop.

## Range literals

Bounded range literals use Rust-shaped syntax. `start..end` creates an exclusive `Range`, and
`start..=end` creates an inclusive `RangeInclusive`. The first implementation supports `i32`
endpoints and requires the corresponding standard-library range types to be imported. Both range
types implement `Iterable<type Item: i32>` through standard-library code, so `for value in 0..10` works when
`std/src/range.gust` is part of the loaded module graph. Open-ended and full ranges are not implemented
yet.

## Literal match patterns

Match literal patterns support `string`, `bool`, and integer primitives. String patterns compare by
value equality. Integer literal and bounded range patterns use the matched integer type for
validation and executable lowering, and integer matches still require a wildcard branch because
coverage is treated conservatively. Bool matches are exhaustive when both `true` and `false` are
covered. Floating-point match patterns are intentionally rejected rather than lowered as exact
comparisons. Char patterns are deferred until the language settles char equality and diagnostics.

## Numeric literals

Integer literals default to `i32`. Decimal and exponent-form literals default to `f64`.
Contextual typing allows integer literals to initialize any numeric type and floating-point
literals to initialize `f32` or `f64`.

Floating-point types support the same implemented numeric operators as integer types: arithmetic,
remainder, comparisons, unary negation, and increment on mutable bindings. `&`, `|`, `^`, `<<`,
and `>>`, including their compound assignment forms, operate only on integer types. Floating-point
equality follows IEEE semantics, including `NaN != NaN`.

The executable backend maps `i128` and `u128` to the C compiler's 128-bit integer extension.

## Numeric casts

`value as Type` supports explicit casts between numeric primitive types, from `char` to integer
types, and from `u8` to `char`. Unsuffixed integer literals may be contextually typed as `u8` for
`as char`, matching Rust behaviour for expressions such as `65 as char`. Other nonnumeric source or
target types are rejected.

Numeric casts follow Rust semantics. Integer-to-integer casts preserve same-width bit patterns,
truncate when narrowing, sign-extend signed values when widening, and zero-extend unsigned values
when widening. Float-to-integer casts round toward zero, convert `NaN` to `0`, and saturate values
outside the integer type's range to that type's minimum or maximum. Integer-to-float and
float-to-float casts produce the closest representable value, rounding ties to even when needed;
overflow produces infinity with the input sign. `char as integer` casts the Unicode scalar value's
code point and then applies the numeric cast rules. `u8 as char` produces the corresponding code
point.

## Numeric string conversion

Every numeric primitive has a built-in `toString(): string` method. It is an intrinsic member,
not an extension function supplied by a prelude, so it is available without imports and takes
precedence over extension functions with the same name.

Integer values use base-10 formatting. Floating-point values use round-trippable formatting with
9 significant digits for `f32` and 17 significant digits for `f64`. The executable backend lowers
numeric conversion to type-specific `gust_rt_*_to_string` runtime helpers. Returned strings are
allocated through `gust_rt_alloc` so allocation remains isolated for the future garbage collector.

## Struct field mutation

Empty structs and empty struct literals are valid. The executable backend may add private padding
when emitting C for a struct with no Gust fields, because standard C structs cannot be truly empty.
That padding is not visible to Gust programs and must not be modeled as a source-level field.

Structs are managed reference values.
Assignment and parameter passing copy references, so aliases observe the same mutations

`mut` grants deep mutation capability through a binding

Mutable references may be used as immutable views, but immutable references cannot become mutable.

Mutable parameters mutate the caller-visible object.

Thread safety is currently the programmer's responsibility.

`.clone()` explicitly deep-clones a struct graph, preserving cycles and repeated references.
The clone is independent and may initialize a mutable binding.

Strings are immutable and may be shared between a value and its clone.

Struct fields may be marked `internal`, such as `internal storage: RawBuffer<T>`. Internal fields
remain readable anywhere the field is visible, but mutable access to them is restricted to direct
methods and static methods declared inside the owning struct. Extension functions and trait impl
methods do not receive this privilege, even when they have `mut self`.

The restriction applies to direct assignment, compound assignment, increments, struct literals, and
mutable capability through the field. For example, code outside `ArrayList<T>` cannot assign
`list.count`, initialize `ArrayList<T> { count: ... }`, pass `list.storage` to a mutable parameter,
bind `let mut storage = list.storage`, or call a `mut self` method through `list.storage`. This
lets collection implementations expose readable state when useful while preserving representation
invariants behind their own methods.

## Struct methods

Struct methods use an implicit immutable `self` parameter whose type is the enclosing struct.
Method calls are statically dispatched and lower to functions with the receiver as their first
argument.

Methods that mutate receiver state declare `mut self` in their parameter list, without a type
annotation. The receiver does not count as a call argument. Calling such a method requires a
mutable-capable receiver.

The method name `clone` is reserved for the built-in deep clone operation.

## Enum methods

Enum methods follow the same receiver rules as struct methods. Instance methods use an implicit
immutable `self` parameter whose type is the enclosing enum, and `Self` refers to that enum inside
instance and static methods. Method calls are statically dispatched and lower to functions with the
receiver as their first argument.

Generic enum methods are monomorphized with the enclosing enum specialization, so methods such as
`Option<T>.unwrapOr(fallback: T): T` become concrete methods like `Option<i32>.unwrapOr`.

## Match payload mutability

Enum payload patterns may contain nested patterns, such as
`Option.Some(Result.Ok(value))`. Payload binding is a real pattern form rather than a special case
of enum variants, so `Option.Some(value)` binds the payload, `Option.Some(_)` discards it without
creating a local, and nested bindings are scoped to the match branch body.

Enum payload bindings may use `mut`, such as `Option.Some(mut value)` or
`Option.Some(Result.Ok(mut value))`. Mutable payload bindings are only valid when the matched value
has mutable capability. This keeps immutable enum views from creating mutable access to nested
managed values while allowing mutable enum methods to mutate struct payloads through `match self`.

Nested enum payload patterns are type-checked recursively. A nested variant must belong to the
payload enum it is matching. Executable lowering compiles patterns into a match decision tree of tag/literal/range
tests, temporary bindings, and branch bodies. Nested payloads and struct fields are
bound to stable temporaries after their checks pass, so the matched expression is
evaluated once and bindings do not re-traverse nested access paths.

## Match decision tree lowering

Executable matches lower through an internal decision representation before C emission.
The tree can test enum tags, struct field subpatterns, literals, ranges, and guards.
Successful checks bind payloads and fields to temporaries that later tests and branch
bodies use. The matched value is evaluated once into a match temporary; nested patterns
do not re-evaluate that expression.

Match checking uses a usefulness-style algorithm (in the spirit of Maranget / Rust). Relative to
the unguarded patterns already seen, each new pattern must match at least one previously uncovered
value; otherwise the branch is unreachable. A match is exhaustive when a wildcard would not be
useful against those patterns. Or-pattern alternatives are checked the same way, so a redundant
alternative inside `|` can be reported even when the overall branch is still useful.

Guarded branches are still checked for usefulness against prior unguarded coverage, but they are
not added to the covered set, so a guard cannot make a match exhaustive by itself. Nested enum
payloads, struct patterns (including `...`), bools, and or-patterns participate in coverage.
Integer and string matches remain non-exhaustive without a wildcard or other covering binding,
because those types are treated as infinite for coverage purposes. Non-exhaustive diagnostics
include a representative missing pattern when one can be constructed.

## Or match patterns

Match patterns may combine alternatives with `|`, such as
`Status.Ready | Status.Waiting`. Or-pattern alternatives are validated as separate patterns
against the same matched value and then merged into one branch scope. Every alternative must bind
the same names with the same mutability and compatible types, so a shared binding can be used in
the branch body regardless of which alternative matched.

Executable lowering emits or-pattern alternatives as sequential attempts that share
predeclared binding temporaries. When any alternative matches, the branch body runs
with those bindings; otherwise the decision tree continues to the next branch.

## Struct match patterns

Struct patterns use Rust-shaped field extraction syntax, such as
`Person { name, age }` and `Person { name: personName, ... }`. A shorthand field is equivalent to
`field: field`. Field entries are checked against the matched struct, duplicate and unknown fields
are rejected, and omitting a field without `...` is an error. Field subpatterns are type-checked
against the declared field type.

Struct patterns bind fields into the match branch scope. They can appear anywhere a nested pattern
is accepted, including enum payloads such as `Option.Some(Person { name, ... })`. Executable
lowering binds each extracted field to a temporary after outer checks pass, so branch bodies use
stable locals rather than repeated field-access expressions.

## Match guards

Match branches may include an `if` guard after the pattern, such as
`Person { name, age } if age >= 18 => name`. The guard is a boolean expression evaluated only after
the pattern matches, and it may use bindings introduced by that pattern. Non-boolean guards are
rejected.

Guarded branches do not count toward exhaustiveness: they are checked for usefulness against prior
unguarded coverage, but they are not added to the covered set, because the guard may fail at
runtime. A later unguarded branch is still required for any value the guarded pattern alone would
otherwise cover.

Executable lowering evaluates the guard only after pattern tests succeed and their
bindings are in scope. A failed guard continues the decision tree at the next branch.
A wildcard or otherwise unconditional pattern with a guard still emits an `if` whose
condition is only the guard.

## Extension functions

An extension function is declared at the top level with `fn Type.functionName(...)`.
It has an implicit immutable `self` parameter of the extended type and is statically dispatched.
An extension function may similarly declare `mut self` when it mutates receiver state.

Extension targets may be parameterized, such as `fn Box<T>.get(): T`, and the extension body may
use those receiver type parameters. Bounds on receiver parameters are written in the target type,
such as `fn Box<T: Named>.name(): string`. A concrete target such as `fn Box<i32>.label(): string`
extends only that instantiation. Static extensions use the same target syntax and may also declare
function type parameters, such as `static fn Box<T>.pair<U>(value: T, other: U)`.

Extension functions do not become members of the extended type. They are available only in the
module where they are declared and in modules that explicitly import them. Importing or otherwise
making a type available does not make its extension functions available. A module may extend a
type declared by another module.

Real type members take precedence over extension functions with the same name. The extension
function name `clone` is reserved for the built-in deep clone operation.

## Traits

The first trait implementation supports concrete trait declarations and `impl Trait for Type`
blocks with instance and static methods. Trait impl instance methods are statically dispatched on
concrete receiver types and lower like methods whose first argument is the receiver. Trait impl
static methods are called through the concrete type and lower like static functions.

Trait instance methods may require `mut self`. Calling such a trait impl method requires the same
mutable receiver capability as calling a mutable struct method.

Impl method return types may be omitted when the trait method declares a return type. The trait
method return type is used as the expected type for the impl body and for method-call typing.

Trait declarations, receiver type declarations, and impl blocks are order-independent within the
loaded module graph.

Real type members take precedence over extension functions, and extension functions take
precedence over trait impl methods with the same receiver and method name. Static functions follow
the same precedence: real static functions, then static extensions, then static trait impl methods.

Generic trait declarations and impls may use type parameters and bounds.

Generic trait methods use ordinary type inference to select a concrete trait specialization from
the receiver, arguments, and expected return type. This supports conversion methods without
compiler knowledge of trait or method names.

`From<T>`, `Into<T>`, and the bounded blanket impl `impl<T, U: From<T>> Into<U> for T` belong in the
Gust standard library and are not compiler built-ins. The conversion does not imply Rust-style move
semantics; it is an explicit typed conversion under Gust's managed-value model.

## Trait-typed values and dynamic dispatch

Trait names may be used as value types. A concrete struct or enum value can initialize a trait-typed
binding, return value, or argument when that type implements the trait. Method calls on
trait-typed values dispatch dynamically through the trait's instance-method vtable.

The executable backend represents a trait-typed value as a fat value containing an erased concrete
value pointer plus a pointer to a trait vtable. Each `impl Trait for Struct` and
`impl Trait for Enum` emits one vtable and small thunks that cast the erased `void* self` pointer
back to the concrete receiver before calling the existing statically lowered trait impl function.
Struct trait objects store the existing managed struct pointer. Enum trait objects box a copy of
the enum value with `gust_rt_alloc` so the erased pointer remains stable.

Static trait methods remain available only through concrete types.

## Static functions

A static function declared inside a type uses `static fn functionName(...)`. It is called through
the type, does not receive `self`, and is available wherever the type is available.

A static extension function uses `static fn Type.functionName(...)`. Like an instance extension,
it is available only where it is declared or explicitly imported and does not become a member of
the extended type.

`Self` refers to the enclosing or extended type inside both static and instance functions. Real
static functions take precedence over static extension functions with the same name.

## Generic structs and enums

Structs may declare type parameters, such as `struct Box<T>`. Type parameters are available in
fields and methods, and every concrete use is monomorphized before semantic analysis and executable
lowering. Different concrete arguments therefore produce distinct struct and method definitions.
Concrete methods are instantiated on demand from method call sites; unused generic methods are not
validated or emitted.

Return types may be omitted from generic instance and static methods. They are inferred for each
concrete specialization before method reachability is resolved, including calls between inferred
generic methods.

Generic struct literals infer concrete arguments from their fields when every type parameter can be
resolved, so `Box { value: 1 }` produces `Box<i32>`. A concrete type annotation also provides
context, allowing `let value: Box<i32> = Box { value: 1  `.
Explicit arguments remain available as
`Box<i32> { value: 1 }`. Nested generic struct types are supported.

Parameterized types may be used for static calls such as `Box<i32>.new(1)`. Generic static methods
also infer arguments from their parameters, allowing `Box.new(1)`. Expected local and return types
provide arguments when a call has insufficient value information. Generic static methods are
instantiated on demand using the same rules as instance methods.

Inference reports an error when constraints conflict or leave a type parameter unresolved.

Enums may declare type parameters, such as `enum Option<T>`. Concrete enum uses are monomorphized
with payload type substitution and distinct executable layouts. Variant construction accepts
explicit concrete types such as `Option<i32>.Some(1)` and infers type arguments from payloads when
all parameters can be resolved. Expected local, return, function-argument, and enclosing payload
types provide context for payload-free or otherwise ambiguous variants such as `Option.None`.

Match patterns keep the generic source name, such as `Option.Some(value)`. The concrete matched
enum determines the specialization and the substituted payload type of each binding. Nested
generic enum payloads and generic enums imported from local modules use the same rules.

Top-level functions may declare type parameters, such as `fn identity<T>(value: T): T`. Calls infer
concrete arguments from parameter types and the expected return type, or accept explicit arguments
such as `identity<i32>(1)`. Concrete functions are monomorphized on demand, including recursive
calls, and unused generic function bodies are not emitted or validated. Function type parameters
must be unique and appear in the function signature so every specialization can be selected at a
call site.

Omitted generic function return types are inferred symbolically before call-site inference and
then substituted for each concrete specialization. This allows an inferred generic result to
provide constraints to an enclosing generic call, such as `identity(identity("value"))`. Generic
enum variant construction and generic struct literals participate in symbolic return inference,
so functions returning `Option.Some(value)`, `Option<T>.None`, or `Box { value: value }` do not
require annotations.

Traits may declare type parameters, such as `trait Named<T>`. Concrete uses of a generic trait,
including `impl Named<string> for Person` and trait-typed values like `let value: Named<string>`,
are monomorphized before semantic analysis and executable lowering. Each concrete specialization
has its own trait object type and dynamic-dispatch vtable.

Generic impl templates such as `impl<T> Named<T> for Box<T>` are monomorphized when their receiver
type and trait can be resolved to concrete types. The generated concrete impl is validated like an
ordinary impl and participates in static trait-method dispatch and dynamic trait-object dispatch.

Generic extension templates are monomorphized only when selected by a concrete call. A receiver
template such as `fn Box<T>.get(): T` can extend every `Box<...>` instantiation whose declared
bounds hold, while a concrete extension such as `fn Box<i32>.label(): string` is emitted only for
`Box<i32>`. Generic extension function parameters are inferred from call arguments and expected
return type, or supplied explicitly at the call site. Real members still take precedence, and
extension lookup still requires the extension function itself to be in scope through declaration or
named import.

Trait impls follow Rust-style overlap rules. Two impl declarations are rejected when their trait
and receiver types can be unified, including blanket impls that overlap only for a future concrete
specialization. Bounds do not make otherwise-overlapping impls disjoint because a type may satisfy
multiple bounds. Gust does not support specialization, so a concrete impl may not overlap a more
general blanket impl.

Bounds are written inside type parameter lists, such as `fn getName<T: Named>(value: T): string`,
`struct Box<T: Clone>`, or `impl<T: Named<string>> Display for Box<T>`. Multiple bounds use `+`.
Concrete specializations must satisfy their bounds through an available concrete or generated impl.
Bounds remain inline; Gust does not have `where` clause syntax.

## Associated types

Traits may declare associated types with `type Name`, optional direct bounds such as
`type Name: Display`, and optional defaults such as `type Name = string` or
`type Name: Display = string`. Every implementation must define each non-defaulted associated type
exactly once with `type Name: ConcreteType`; omitted defaulted definitions use the trait default.
Associated types are canonical related
types selected by an implementation: they are determined by the trait, its positional generic
arguments, and the implementing type, and may appear in either parameter or return positions. They
do not participate in implementation identity, so two otherwise-overlapping impls cannot be
distinguished by choosing different associated types. Generic impl definitions may refer to impl
type parameters, such as `type Output: T`.

Trait method signatures may project through `Self`, such as `Self.Item`. A bounded type parameter
may project through the uniquely applicable bound with `T.Item`. Projection resolution substitutes
the selected impl definition recursively inside function types, struct fields, enum payloads, and
generic types. A projection is rejected when no applicable trait or implementation declares it, or
when multiple bounds or applicable implementations make the associated-type name ambiguous.

Associated-type equality bindings are marked with `type` and written alongside positional trait
arguments, for example `Producer<type Item: i32>` and
`Index<usize, type Output: string>`. The marker distinguishes implementation-selected associated
types from caller-selected named generic arguments. A trait-typed value must bind every associated
type needed to determine its instance-method signatures. Those bindings are part of the specialized
trait-object type and vtable signature, but remain outside impl coherence identity.

Generic associated types declare their own parameters, such as `type Item<T>`, and implementations
define them with the matching arity, such as `type Item<T>: Option<T>`. They are projected with
ordinary type arguments, for example `Self.Item<i32>` or `T.Item<string>`.

Monomorphization resolves associated-type projections before semantic analysis and executable
lowering. Generic impl associated-type definitions are first substituted with the concrete impl
arguments; trait methods are then specialized with both positional arguments and associated-type
bindings. Module rewriting preserves declarations, definitions, bindings, and projections so the
same process applies across local-module boundaries. Associated-type equality bindings remain
limited to non-generic associated types, so generic associated types are currently statically
dispatched rather than carried through trait-object vtables.

`Iterator`, `Iterable`, `FromElements`, and `FromIterator` use an associated `Item` type instead
of a positional element type parameter. The standard-library `Index<Key>` trait exposes its read
type through `Output`, while `IndexSet<Key>` exposes its assigned type through `Value`.

## First-class functions

Function values are represented as closure pairs: an environment pointer plus a call pointer.
Lambdas capture local bindings by shared cell, not by value snapshot, so mutations through the
closure and mutations in the enclosing scope observe the same binding. The executable backend
allocates captured cells and closure environments through `gust_rt_alloc` so the implementation can
move to managed allocation when the runtime garbage collector is introduced.

The executable backend supports concrete function types. Lambda parameters are inferred from a
function type context, or otherwise require annotations. Lambda return types are inferred from
expression bodies and from consistent block returns when no return type is annotated. Generic
functions specialize closure function types, captured `let` cells, closure environments, and
indirect-call signatures with their concrete type arguments. A generic function used as a value is
specialized from an expected concrete function type, or requires explicit type arguments. Captured
parameters are left for a later implementation step.
