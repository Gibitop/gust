# Gust language design

Gust is a compiled general purpose programming langugage designed for writing backend services and CLI applications

## Language 
- Syntax similar to rust
- Type system similar to rust
- First class Option and Error types
- Traits like rust
- Errors as values with special syntax to propogate the error (like rusts `?` operator)
- Optional chaining like typescript
- Pattern matching like rust
- Comptime system like zig (for values and types)
- Type utils like typescript (like Pick, Omit, Parameters, ReturnType, Indexed Access to fields  etc)
- Mapped types like typescript
- Has GC like go. No manual memory management, no borrow checker
- Has light threads like go with goroutines
- Type inference where possible like typescript
- Immutability by default (especially for function parameters)
- Async support like rust tokio
- Module system like ESM (with only named exports)
- Each function parameter can be passed by name like in python
- Reflection
- Decorators
- Struct, function, parameter and field attributes like rust
- Defer like in go
- Rich and simple standart library like go
- Template literals like in typescript
- Compiles to a self-contained binary like go
- The compiled binary includes a small runtime like go
- Foreign function interface support from importing libraries from other languages
- Module reflection: 
    - Get list of modules (to build file-based routing for example)
    - Get list of members of each module
    - Load non exported members (for unit testing for example)


## Toolchain
- Has good compiler warnings and errors like rust
- Builds fast like go
- Package manager like pnpm
- First party testing suit
- Linter and formatter


## IDE support
- LSP for IDE support
- Syntax highlighting in IDEs
- Debugging tools

