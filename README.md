# Bloop

`bloop` is an LLVM-compiled programming language. It started as a test project to learn the basics of Rust. Its syntax is almost a subset of Rust; I might have called it `Rust--` instead.

[Check out the web-based REPL/compiler sandbox](https://wessles.github.io/bloop-lang/)

My priorities for this project:

0. _**Fun.**_
1. _**Learn Rust.**_ Rust is shiny and exciting, enough so that I'll voluntarily stare at a screen for a few more hours after a full work day.
2. **_Explore language design._** Languages and compilers always interested me, but I never got past the AST-crawling / bytecode interpreter stage in past projects. I was particularly interested in LLVM, linking and type checking.
3. **_Try out AI._** After writing a good foundation myself, I used this project to experiment with Claude Code. See [AI Usage](#ai-usage).

## Pipeline

```
tokens::tokenizer -> ast::parser -> ast::type_checker -> blir::blir_lowering -> BLIRCompileUnit
```

From there, execution takes one of two paths:

- **Interpret** (`-r`, or no flag): run the `BLIRCompileUnit` directly via `blir_interpreter`.
- **Compile** (`-c`/`-e`): lower further through `llir_generator` into real LLVM IR via
  `inkwell`, then write a `.ll` file or JIT-execute it.

## Syntax

**1. Functions, `let`, and `print`.** A function's return value is the value of its last
expression (no `return` keyword, mirroring Rust's tail-expression rule). `let` infers its type
from the right-hand side unless you annotate it.

```
fn add(a: i64, b: i64) {
    a + b
}

fn main() {
    let sum = add(2, 3);   // type inferred as i64
    print sum;             // prints 5
    0
}
```

**2. Arithmetic, comparisons, and compound assignment.** `+ - * / ^` (`^` is exponentiation, not
XOR), the usual comparisons (`== != < <= > >=`), and `+= -= *= /= ^=`.

```
fn main() {
    let x: i64 = 2;
    x *= 5;
    print x ^ 2;   // prints 100
    print x >= 10; // prints 1 (booleans print as 1/0)
    0
}
```

**3. `if`/`while`/`for`.** `if` (with a required `else`) evaluates to a value like Rust's tail
`if`, as long as it sits in tail position of a block -- most naturally as a function's last
expression. `while` and C-style `for(init; cond; update)` are statements. Recursion works,
including mutual recursion between sibling functions.

```
fn is_even(n: i64) {
    if (n == 0) { true } else { is_odd(n - 1) }
}
fn is_odd(n: i64) {
    if (n == 0) { false } else { is_even(n - 1) }
}

fn label(even) {
    if (even) { "even" } else { "odd" }
}

fn main() {
    print label(is_even(10));   // prints "even"

    for (let i = 0; i < 3; i += 1) {
        print i;                // prints 0, 1, 2
    }
    0
}
```

**4. Modules and `use`.** `mod name { .. }` groups declarations under a qualified name
(`name::item`); `mod name;` alone is a forward declaration with no body. `use` brings a module's
items into unqualified scope, resolved fully at type-check time.

```
mod math {
    fn square(x: i64) { x ^ 2 }
}

use math;

fn main() {
    print math::square(4); // qualified form always works
    print square(5);       // unqualified, thanks to `use math;`
    0
}
```

## Building

```bash
cargo build                     # build (default features: cli + llvm)
cargo build --no-default-features --features cli   # build without LLVM
```

The `llvm` feature needs a system LLVM 20 install. The `wasm` feature (opt-in) is mutually
exclusive with `llvm`, since `inkwell` can't target `wasm32`.

## Usage

The default action, `cargo run -- file.blp`, type-checks the file and runs `main` straight
through the `BLIRInterpreter` -- no LLVM install required. If you want a real, standalone
executable instead, compile to LLVM IR with `-c` and hand the resulting `.ll` file to `clang` (or
`llc` + a linker) the same way you would any other LLVM frontend's output:

```bash
cargo run -- -c hello.blp        # writes hello.blp.ll
clang hello.blp.ll -o hello      # link it into a native executable
./hello
```

`-e` skips the file entirely and JIT-compiles/runs the same LLVM IR in-process, which is faster
to iterate with than round-tripping through `clang` while you're still changing the program.

```bash
cargo run                       # no file given -- launches the terminal REPL
cargo run -- file.blp           # run `main` through the BLIR interpreter (default action)
cargo run -- -r file.blp        # same as the default, spelled explicitly
cargo run -- -b file.blp        # print the lowered BLIR to stdout (no LLVM needed)
cargo run -- -c file.blp        # compile to LLVM IR, writing file.blp.ll
cargo run -- -e file.blp        # JIT-compile via LLVM and execute
```

## AI Usage

First off: none of this section used AI; I prefer to write personal thoughts myself. If you see an emdash, know I put it there myself.

_Opinions are my own and do not represent the views of my employer._

### Reasons

Because the original purpose of this project was to learn Rust, I didn’t let an agent touch the code until I had a good grasp of Rust’s features. Before using an agent, I finished the Rust Book and wrote a full AST-walking interpreter (which I later stripped out in favor of a unified IR interpreter).

I like writing code myself: half for the fun of it, half for my ego. But I have a busy life, and I don’t want to spend my free time typing. In a project where even simple features can require hours of busywork, I was happy to offload some of that to an LLM.

### How I used it

I kept my prompts targeted and architectural to make sure I owned all design decisions. I also thoroughly reviewed all code, making sure I fully understood and approved before committing; getting this right usually took many prompting iterations. I didn’t write most of the code here by hand (much of my original code was lost in refactors), but if someone asked me about any structure or file in this project, I could explain what it does.

I found Claude most useful for refactors, drafting features / commit summaries, writing unit tests, changing config files, and hooking up libraries.

I avoided vibe-coding for all core language features — I define that as allowing the agent to commit code without going through a detailed review process. But since this project is for fun, I occasionally loosened my review for extras. For example, I largely vibe-coded the contents of `www/`, because the point of this project is learning about Rust and language design, not building web apps. I had already written a console-based interpreter by hand and just wanted it on a website so I could show a friend. Took me 20 minutes to spin up.

Overall Claude Code saved me a lot of time on this project, and I’ll probably use it in the future.

... but as useful as Claude is, I do feel some ickiness in giving my money to Anthropic; [AI isn't great for the environment](https://www.nytimes.com/2024/05/06/business/dealbook/ai-power-energy-climate.html), and my money is certainly being used to build more data centers. For my own conscience, as long as I pay for Claude Code, I'll also donate $20 a month to the [Clean Air Task Force](https://give.catf.us/campaign/756949/donate).
