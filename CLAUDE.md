# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                     # build (default features: cli + llvm)
cargo run                       # no file given -- launches the terminal REPL
cargo run -- file.blp           # run `main` through the BLIR interpreter (the default action)
cargo run -- -r file.blp        # same as the no-flag default, spelled explicitly
cargo run -- -b file.blp        # print the lowered BLIR (debug-formatted) to stdout -- works without LLVM installed
cargo run -- -c file.blp        # compile to LLVM IR, writing file.blp.ll
cargo run -- -e file.blp        # JIT-compile via LLVM and execute
cargo test                      # run all tests
cargo test type_checker         # run type checker tests only
cargo test blir_lowering        # run AST -> BLIR lowering tests only
cargo test blir_interpreter     # run BLIR interpreter tests only
cargo test blir_linker          # run BLIR linking tests only
cargo test llir_generator       # run BLIR -> LLVM IR tests only
cargo clippy                    # lint
```

Cargo features: `cli` (native binary, on by default, needs `crossterm` -- pulled in for `repl/`), `llvm` (compile
path, on by default, needs `inkwell`/a system LLVM 20 install), `wasm` (opt-in, mutually exclusive with `llvm`
since inkwell can't target `wasm32`). `cargo build --no-default-features --features cli` builds without LLVM.

**REPL.** `repl/terminal_repl.rs` drives a raw-mode terminal session (line editing, history via `repl/history.rs`)
against `ast::interpreter::Interpreter`, the REPL's session wrapper. `BLIRInterpreter` (`blir_interpreter/
interpreter.rs`) is stateful: `link(BloopLibrary)` merges a library's functions into its linked table (erroring on
a name clash, leaving it untouched), and `execute_entry_point(name)`/`execute_ops(ops, registers, output)` run
against everything linked so far. Each submission compiles *independently* (not concatenated with prior source):
its `fn`/`mod`/`use` items get `link`ed (so redefining a function under an earlier submission's name is a hard
error); everything else (`let`s, bare expressions, `print`s) is wrapped, purely to reuse `blir_lowering`, in a
throwaway driver function under a unique per-submission name that's never linked or called -- its ops instead run
directly against the session's own long-lived registers (`BLIRInterpreterState`, `blir_interpreter/state.rs`, has
owned `String`-keyed registers precisely so this works), the same way a `let` inside one ordinary function body
stays bound for the rest of that body. That's what lets a `let` from one submission stay bound in a later one,
since BLIR itself has no notion of a global variable -- only functions are linkable. A submission's own function
declarations stay linked even if its bare statements then fail at runtime; everything else about a failing
submission (parse/type-check/runtime) leaves the session untouched. `repl/wasm_repl.rs` (behind the `wasm`
feature, for the browser demo in `www/`) mirrors the same `Interpreter`; `blir`/`blir_interpreter` are gated behind
`any(cli, wasm)` so both native and wasm builds see it.

## Architecture

Bloop is an interpreted/compiled language with `.blp` source files. There is one pipeline:

**Front-end:** `tokens::tokenizer` → `ast::parser` → `ast::type_checker` → `blir::blir_lowering` (typed AST →
`BLIRCompileUnit`, no LLVM needed -- this is what `-b` prints)

From there, `-r` (or no flag) executes it directly via `blir_interpreter::BLIRInterpreter::execute`; `-c`/`-e`
instead lower it further through `llir_generator::llir_compilation` (`BLIRCompileUnit` → real LLVM IR via
`inkwell`, needs the `llvm` feature) to write a `.ll` file or JIT-execute it.

The crate is split into a lib (`src/lib.rs`) and a thin native binary (`src/main.rs`, requires the `cli` feature).
`cli::run_cli` parses args and drives every mode; BLIR lowering always runs first (needed by every mode), and
interpreting via `BLIRInterpreter` is the default action when none of `-b`/`-c`/`-e` is given.

`AST` (in `ast/mod.rs`) is the central struct that owns tokens and typed expressions and exposes `compile_src`
(and `compile_src_with_externals`, for type-checking one file against another already-compiled file's exported
signatures -- see `blir_linker.rs`).

`blir` (behind the `cli` feature; its main consumer is `cli.rs`) is the BLIR compile backend -- entirely
LLVM-independent. `blir_program.rs` defines `BLIRCompileUnit`/`BLIRFunction`/`BLIROperation`, a lower-level IR
separate from the AST layer; `blir_lowering.rs` lowers typed AST into it. `blir_linker.rs` merges several
`BLIRCompileUnit`s (e.g. one per source file) into one `BLIRExecutable`, resolving cross-unit `CallFunction`
targets.

`blir_interpreter` (also behind the `cli` feature, since it depends on `blir::blir_program`) directly executes a
`BLIRCompileUnit`'s `main` -- a program-counter walk over its flat `BLIROperation` list with a label→index map for
jumps, recursing on Rust's own call stack for `CallFunction`. It only supports what BLIR itself can represent: no
tuples, no first-class functions (BLIR has no aggregate type and only calls functions by static name).

`llir_generator` (behind both the `cli` and `llvm` features) lowers a `BLIRCompileUnit` into real LLVM IR via
`inkwell`. `llir_program.rs` and `llir_compilation.rs` can JIT-execute the result or write it to a `.ll` file.
Function calls (including recursion) and if-as-value expressions are both supported end to end.

Tests live alongside their modules: `src/ast/type_checker_tests.rs`, `src/blir/blir_lowering_tests.rs`,
`src/blir/blir_linker_tests.rs`, `src/blir_interpreter/mod_tests.rs`, `src/llir_generator/llir_convert_tests.rs`
(LLVM IR text assertions), and `src/llir_generator/llir_execution_tests.rs` (JIT-executes and checks the actual
return value).

## Rules

1. Do not ever run `cargo fmt`
2. Keep comments succinct, 1 to 4 lines unless you REALLY need to convey something
3. Error enums should either be empty or contain enums, not string messages