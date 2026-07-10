# Development decisions log

## First toolchain

First toolchain will be implemented in the rust programming language. Later the toolchain will be ported to the gust language itself

## Gust project structure

Minimal gust project contains a single `.gust` file with a `main` function

## Modules

Local modules use relative paths and named imports. A relative import without an extension resolves
to a `.gust` file next to the importing module. Top-level declarations are available for named
import; explicit export visibility will be introduced separately. Imported names can be bound to a
different local name with `from ./module import { original as localName }`.

An unbraced import binds the module as a namespace:
`from ./module import namespace`. Declarations are then accessed through the namespace, such as
`namespace.function()` or `namespace.Struct { value: 1 }`. Extension functions must still be
imported by name so their availability remains explicit at method call sites.

The compiler loads the complete local module graph before semantic analysis. Imported modules use
deterministic internal names derived from their import path so declarations with the same source
name in different modules do not collide. Only names listed by an importing module are added to
that module's scope. Extension functions follow the same rule and retain real-member precedence.

Package module resolution is not implemented yet. Import cycles are rejected.

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

## String memory management

Gust will use garbage collection for managed values, including strings. Do not introduce ownership or lexical `free` semantics for strings as an interim design.

The current C backend may temporarily leak heap-allocated string concat results. Keep allocation isolated behind Gust-shaped runtime helpers, so raw `malloc` usage can later be replaced by GC allocation.

## Runtime development

Generated C should route operations that will later be runtime-managed through Gust-shaped helpers instead of calling C primitives directly.

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

## While loops

`while` conditions must be boolean. A `while` body has block scope, so bindings declared inside
the body do not escape. `break` and `continue` are statements and may only be used inside loop
bodies. Executable builds lower `while`, `break`, and `continue` directly to C control flow.
Iterable `for` loops remain separate and will be implemented once collection and iterator
semantics are available.

## Numeric literals

Integer literals default to `i32`. Decimal and exponent-form literals default to `f64`.
Contextual typing allows integer literals to initialize any numeric type and floating-point
literals to initialize `f32` or `f64`.

Floating-point types support the same implemented numeric operators as integer types: arithmetic,
remainder, comparisons, unary negation, and increment on mutable bindings. `&`, `|`, `^`, `<<`,
and `>>`, including their compound assignment forms, operate only on integer types. Floating-point
equality follows IEEE semantics, including `NaN != NaN`.

The executable backend maps `i128` and `u128` to the C compiler's 128-bit integer extension.

## Numeric string conversion

Every numeric primitive has a built-in `toString(): String` method. It is an intrinsic member,
not an extension function supplied by a prelude, so it is available without imports and takes
precedence over extension functions with the same name.

Integer values use base-10 formatting. Floating-point values use round-trippable formatting with
9 significant digits for `f32` and 17 significant digits for `f64`. The executable backend lowers
numeric conversion to type-specific `gust_rt_*_to_string` runtime helpers. Returned strings are
allocated through `gust_rt_alloc` so allocation remains isolated for the future garbage collector.

## Struct field mutation

Structs are managed reference values.
Assignment and parameter passing copy references, so aliases observe the same mutations

`mut` grants deep mutation capability through a binding

Mutable references may be used as immutable views, but immutable references cannot become mutable.

Mutable parameters mutate the caller-visible object.

Thread safety is currently the programmer's responsibility.

`.clone()` explicitly deep-clones a struct graph, preserving cycles and repeated references.
The clone is independent and may initialize a mutable binding.

Strings are immutable and may be shared between a value and its clone.

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

Enum payload patterns may bind the payload with `mut`, such as `Option.Some(mut value)`.
Mutable payload bindings are only valid when the matched value has mutable capability. This keeps
immutable enum views from creating mutable access to nested managed values while allowing mutable
enum methods to mutate struct payloads through `match self`.

## Extension functions

An extension function is declared at the top level with `fn Type.functionName(...)`.
It has an implicit immutable `self` parameter of the extended type and is statically dispatched.
An extension function may similarly declare `mut self` when it mutates receiver state.

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
including `impl Named<String> for Person` and trait-typed values like `let value: Named<String>`,
are monomorphized before semantic analysis and executable lowering. Each concrete specialization
has its own trait object type and dynamic-dispatch vtable.

Generic impl templates such as `impl<T> Named<T> for Box<T>` are monomorphized when their receiver
type and trait can be resolved to concrete types. The generated concrete impl is validated like an
ordinary impl and participates in static trait-method dispatch and dynamic trait-object dispatch.

Trait impls follow Rust-style overlap rules. Two impl declarations are rejected when their trait
and receiver types can be unified, including blanket impls that overlap only for a future concrete
specialization. Bounds do not make otherwise-overlapping impls disjoint because a type may satisfy
multiple bounds. Gust does not support specialization, so a concrete impl may not overlap a more
general blanket impl.

Bounds are written inside type parameter lists, such as `fn getName<T: Named>(value: T): String`,
`struct Box<T: Clone>`, or `impl<T: Named<String>> Display for Box<T>`. Multiple bounds use `+`.
Concrete specializations must satisfy their bounds through an available concrete or generated impl.
Bounds remain inline; Gust does not have `where` clause syntax.

## First-class functions

Function values are represented as closure pairs: an environment pointer plus a call pointer.
Lambdas capture local bindings by shared cell, not by value snapshot, so mutations through the
closure and mutations in the enclosing scope observe the same binding. The executable backend
allocates captured cells and closure environments through `gust_rt_alloc` so the implementation can
move to managed allocation when the runtime garbage collector is introduced.

The first executable-backend implementation supports monomorphic function types. Lambda parameters
are inferred from a function type context, or otherwise require annotations. Lambda return types are
inferred from expression bodies and from consistent block returns when no return type is annotated.
Captured `let` locals are supported; captured parameters and generic closure values are left for a
later implementation step.
