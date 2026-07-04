# Development decisions log

## First toolchain

First toolchain will be implemented in the rust programming language. Later the toolchain will be ported to the gust language itself

## Gust project structure

Minimal gust project contains a single `.gust` file with a `main` function

## String memory management

Gust will use garbage collection for managed values, including strings. Do not introduce ownership or lexical `free` semantics for strings as an interim design.

The current C backend may temporarily leak heap-allocated string concat results. Keep allocation isolated behind Gust-shaped runtime helpers, so raw `malloc` usage can later be replaced by GC allocation.

## Runtime development

Generated C should route operations that will later be runtime-managed through Gust-shaped helpers instead of calling C primitives directly.

## Generated C naming

Generated C reserves `gust_rt_*` for runtime helpers. User-defined symbols must not use this prefix.

User-defined functions should use deterministic internal names shaped like `gust_fn_<hash>_<source_name>` with a nearby comment containing the original Gust function name.

User-defined structs should use deterministic internal names shaped like `gust_struct_<hash>_<source_name>` with a nearby comment containing the original Gust struct name.

Generated local variables and struct fields should use `gust_<source_name>`. Keep source-name suffixes sanitized so generated identifiers stay valid C identifiers.


## Equality

`==` and `!=` compare numeric and boolean values directly. Strings compare by value through a Gust runtime helper, never by backend pointer identity.

Struct and enum equality will be introduced with trait-based equality rather than receiving implicit field-by-field semantics.
