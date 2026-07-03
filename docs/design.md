# Gust language design

Gust is a compiled general purpose programming language designed for writing backend services and CLI applications

## Features
- Syntax similar to rust
- Type system similar to rust
- First class Option and Error types
- Traits like rust
- Pattern matching like rust
- Has GC like go. No manual memory management, no borrow checker
- Has light threads like go with go routines
- Immutability by default (especially for function parameters)
- Module system like ESM (with only named exports)
- Compiles to a self-contained binary like go
- The compiled binary includes a small runtime like go
- Has good compiler warnings and errors like rust

