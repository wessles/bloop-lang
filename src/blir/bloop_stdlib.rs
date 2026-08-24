use crate::ast::TypedAST;
use crate::blir::bloop_library::BloopLibrary;

pub(crate) fn get_stdlib() -> BloopLibrary {
    let bloop_stdlib_src = r#"
mod std {
    fn sqr(x: f64) { x ^ 2 }
}
"#
    .to_string();
    let ast = TypedAST::compile_src(bloop_stdlib_src).expect("bloop_stdlib had a compile error?");
    BloopLibrary::new("libstd", &ast).expect("bloop_stdlib library creation error?")
}
