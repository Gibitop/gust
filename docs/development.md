# Development descisions log

## First toolchain

First toolchain will be implmented in the rust programming language. Later the toolchain will be ported to the gust language itself


## Underlying tools

Gust compiler uses LLVM under the hood


## Gust project structure

Minimal gust project contains a single `.gust` file with a `main` function

A typical gust project contains a `project.yaml` file and a `src` folder with `.gust` source files and other folder containing more source files and folders

The `project.yaml` file contains a `scripts` object that behaves like in npm's `package.json`
