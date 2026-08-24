/// Entry point for the native `bloop` binary (`src/main.rs`): parses CLI
/// args, then interprets/compiles the given files. Lives here (rather than
/// in `main.rs` itself) so it can freely use this crate's internal
/// `ast`/`blir`/`llir_generator` items, which a separate `[[bin]]` crate
/// could only reach if they were fully `pub`.
pub fn run_cli() {
    use crate::ast::AST;
    use std::{env, fs};

    let args: Vec<String> = env::args().collect();
    let mut compile = false;
    let mut execute_llvm_ir = false;
    let mut print_blir = false;
    let mut interpret_blir = false;
    let mut files: Vec<&String> = Vec::<&String>::new();
    for arg in &args[1..] {
        match arg.as_str() {
            "-c" | "--compile-llvm-ir" => compile = true,
            "-e" | "--execute-as-llvm-ir" => execute_llvm_ir = true,
            "-b" | "--print-blir" => print_blir = true,
            "-r" | "--interpret-blir" => interpret_blir = true,
            _file => files.push(arg),
        }
    }
    // Interpreting via BLIR is the default action when no other mode flag
    // is given -- there's no REPL and no tree-walking fallback anymore.
    if !print_blir && !compile && !execute_llvm_ir {
        interpret_blir = true;
    }

    if files.is_empty() {
        if let Err(err) = crate::repl::terminal_repl::repl() {
            eprintln!("{}", err);
        }
        return;
    }

    for file in files {
        let src = fs::read_to_string(file).unwrap_or_else(|e| panic!("{}", e));
        let stdlib_metadata = crate::blir::bloop_stdlib::get_stdlib().m_metadata;
        let program = match AST::compile_src_with_externals(src.clone(), stdlib_metadata) {
            Ok(program) => program,
            Err(err) => {
                eprintln!("{}", err.get_err(src.as_str(), Some("program")));
                continue;
            }
        };

        let library = match crate::blir::bloop_library::BloopLibrary::new(file.as_str(), &program) {
            Ok(library) => library,
            Err(err) => {
                eprintln!("{}", err.get_err(src.as_str(), Some("ir")));
                continue;
            }
        };

        if print_blir {
            println!("{}", library.m_compile_unit.to_pretty_string(&src));
        }

        // Read-only against `library.m_compile_unit`, so these run before
        // `interpret_blir` below, which needs to consume `library` itself
        // (via `link`).
        if compile || execute_llvm_ir {
            #[cfg(feature = "llvm")]
            {
                if compile {
                    compile_llvm_ir(file, &library.m_compile_unit);
                }
                if execute_llvm_ir {
                    jit_execute(file, &library.m_compile_unit);
                }
            }
            #[cfg(not(feature = "llvm"))]
            {
                eprintln!(
                    "{}: this build of bloop was compiled without the `llvm` feature; \
                     -c/-e are unavailable (rebuild with `--features llvm`, the default)",
                    file
                );
            }
        }

        if interpret_blir {
            let mut interpreter = crate::blir_interpreter::interpreter::BLIRInterpreter::new();
            interpreter
                .link(crate::blir::bloop_stdlib::get_stdlib())
                .expect("a freshly constructed interpreter has nothing yet to clash with stdlib");
            let result = interpreter
                .link(library)
                .map_err(|e| format!("{:?}", e))
                .and_then(|()| {
                    interpreter
                        .execute_entry_point("main")
                        .map_err(|e| format!("{:?}", e))
                });
            match result {
                Ok(result) => {
                    print!("{}", result.output);
                    if let Some(value) = result.value {
                        println!("{:?}", value);
                    }
                }
                Err(err) => eprintln!("{}: {}", file, err),
            }
        }
    }
}

#[cfg(feature = "llvm")]
fn compile_llvm_ir(file: &str, ir: &crate::blir::BLIRCompileUnit) {
    use crate::llir_generator::llir_program::LLIRProgram;
    use inkwell::context::Context;
    use std::fs::File;
    use std::io::BufWriter;

    let llvm_context = Context::create();
    let llvm_ir = match LLIRProgram::compile_from_blir(&llvm_context, ir) {
        Ok(llvm_ir) => llvm_ir,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let mut out_path = file.to_string();
    out_path.push_str(".ll");
    let out_file = File::create(out_path.as_str()).unwrap();
    let mut writer = BufWriter::new(out_file);
    llvm_ir.write_to(&mut writer).unwrap_or_else(|err| {
        eprintln!("{}", err);
    });
}

#[cfg(feature = "llvm")]
fn jit_execute(file: &str, ir: &crate::blir::BLIRCompileUnit) {
    use crate::llir_generator::llir_program::LLIRProgram;
    use inkwell::OptimizationLevel;
    use inkwell::context::Context;

    let llvm_context = Context::create();
    let llvm_ir = match LLIRProgram::compile_from_blir(&llvm_context, ir) {
        Ok(llvm_ir) => llvm_ir,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let main_fn = match llvm_ir.m_module.get_function("main") {
        Some(f) => f,
        None => {
            eprintln!("{}: no `main` function defined", file);
            return;
        }
    };
    let execution_engine = llvm_ir
        .m_module
        .create_jit_execution_engine(OptimizationLevel::Aggressive)
        .unwrap();
    unsafe {
        execution_engine.run_function_as_main(main_fn, &[]);
    }
}
