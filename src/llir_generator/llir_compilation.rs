use crate::blir::{
    BLIRArithOp, BLIRCmpOp, BLIRCompileUnit, BLIRConstant, BLIRFunction, BLIRLabel, BLIROperation,
    BLIRRegister, BLIRType,
};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::{Linkage, Module};
use inkwell::values::{BasicValueEnum, FloatValue, FunctionValue, IntValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::hash::{DefaultHasher, Hasher};

#[derive(Debug)]
enum LLIRGenError {
    MissingReturnRegister(String),
    UnsupportedCallTarget(String),
}

impl Display for LLIRGenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LLIRGenError::MissingReturnRegister(fn_name) => write!(
                f,
                "internal compiler error: function `{}` has a scalar return type but no resolvable return value",
                fn_name
            ),
            LLIRGenError::UnsupportedCallTarget(name) => write!(
                f,
                "cannot call `{}` — calling a function value (e.g. a parameter or variable holding a function, rather than a function declared with `fn`) is not yet supported by the LLVM backend",
                name
            ),
        }
    }
}
impl Error for LLIRGenError {}

fn str_global_name(s: &str) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(s.as_bytes());
    format!("str.{}", hasher.finish())
}

// Emits a private, null-terminated C string as a global and returns a
// pointer to it. `text` should not include the null terminator.
fn add_c_string_global<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    text: &str,
    name: &str,
) -> PointerValue<'ctx> {
    let i8_type = context.i8_type();
    let bytes: Vec<u8> = text.bytes().chain(std::iter::once(0)).collect();
    let arr_type = i8_type.array_type(bytes.len() as u32);
    let global = module.add_global(arr_type, None, name);
    global.set_constant(true);
    global.set_linkage(Linkage::Private);
    global.set_unnamed_addr(true);
    let byte_vals: Vec<_> = bytes
        .iter()
        .map(|&b| i8_type.const_int(b as u64, false))
        .collect();
    global.set_initializer(&i8_type.const_array(&byte_vals));
    global.as_pointer_value()
}

fn ir_cmp_to_predicate(op: &BLIRCmpOp) -> IntPredicate {
    match op {
        BLIRCmpOp::Lt => IntPredicate::SLT,
        BLIRCmpOp::Le => IntPredicate::SLE,
        BLIRCmpOp::Gt => IntPredicate::SGT,
        BLIRCmpOp::Ge => IntPredicate::SGE,
        BLIRCmpOp::Eq => IntPredicate::EQ,
        BLIRCmpOp::Ne => IntPredicate::NE,
    }
}

fn ir_cmp_to_float_predicate(op: &BLIRCmpOp) -> FloatPredicate {
    match op {
        BLIRCmpOp::Lt => FloatPredicate::OLT,
        BLIRCmpOp::Le => FloatPredicate::OLE,
        BLIRCmpOp::Gt => FloatPredicate::OGT,
        BLIRCmpOp::Ge => FloatPredicate::OGE,
        BLIRCmpOp::Eq => FloatPredicate::OEQ,
        BLIRCmpOp::Ne => FloatPredicate::ONE,
    }
}

// printf and the format strings `print` uses to render numeric values.
struct Printf<'ctx> {
    func: FunctionValue<'ctx>,
    int_fmt: PointerValue<'ctx>,
    float_fmt: PointerValue<'ctx>,
}

// Lowers `dest = base ^ power` for an integer base into a runtime
// repeated-multiplication loop, mirroring the evaluator's Exponent
// semantics (negative power reciprocates the positive-power result,
// integer division truncates). LLVM has no integer-only pow
// instruction/intrinsic (`llvm.powi`/`llvm.pow` are float-only), and
// round-tripping through `f64` would lose precision past 2^53, so unlike
// `build_pow_intrinsic` below this can't delegate to an intrinsic.
fn build_int_pow<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    fn_val: FunctionValue<'ctx>,
    base: IntValue<'ctx>,
    power: IntValue<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, Box<dyn Error>> {
    let i64_type = context.i64_type();
    let zero = i64_type.const_zero();

    let is_neg =
        builder.build_int_compare(IntPredicate::SLT, power, zero, &format!("{name}.is_neg"))?;
    let neg_power = builder.build_int_neg(power, &format!("{name}.neg_power"))?;
    let abs_power = builder
        .build_select(is_neg, neg_power, power, &format!("{name}.abs_power"))?
        .into_int_value();

    // Not the function's entry block -- just whatever block was active
    // when this op was reached, so the loop's phis know where the
    // pre-loop values (i=0, acc=1) come from.
    let preheader_bb = builder.get_insert_block().unwrap();
    let cond_bb = context.append_basic_block(fn_val, &format!("{name}.cond"));
    let body_bb = context.append_basic_block(fn_val, &format!("{name}.body"));
    let loop_end_bb = context.append_basic_block(fn_val, &format!("{name}.loop_end"));
    let recip_bb = context.append_basic_block(fn_val, &format!("{name}.recip"));
    let done_bb = context.append_basic_block(fn_val, &format!("{name}.done"));

    builder.build_unconditional_branch(cond_bb)?;

    // cond: acc = phi(1, acc * base), i = phi(0, i + 1); loop while i < |power|
    builder.position_at_end(cond_bb);
    let acc_phi = builder.build_phi(i64_type, &format!("{name}.acc"))?;
    let i_phi = builder.build_phi(i64_type, &format!("{name}.i"))?;
    let one = i64_type.const_int(1, true);
    acc_phi.add_incoming(&[(&one, preheader_bb)]);
    i_phi.add_incoming(&[(&zero, preheader_bb)]);
    let i_val = i_phi.as_basic_value().into_int_value();
    let cont =
        builder.build_int_compare(IntPredicate::SLT, i_val, abs_power, &format!("{name}.cont"))?;
    builder.build_conditional_branch(cont, body_bb, loop_end_bb)?;

    // body: multiply and increment, then loop back to cond
    builder.position_at_end(body_bb);
    let acc_val = acc_phi.as_basic_value().into_int_value();
    let next_acc = builder.build_int_mul(acc_val, base, &format!("{name}.mul"))?;
    let next_i =
        builder.build_int_add(i_val, i64_type.const_int(1, false), &format!("{name}.inc"))?;
    let body_end_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(cond_bb)?;
    acc_phi.add_incoming(&[(&next_acc, body_end_bb)]);
    i_phi.add_incoming(&[(&next_i, body_end_bb)]);

    // loop_end: if power was negative, reciprocate; otherwise use the
    // accumulated result as-is (a real branch, not a select, so a
    // reciprocal of a legitimate 0^positive result is never computed).
    builder.position_at_end(loop_end_bb);
    let loop_result = acc_phi.as_basic_value().into_int_value();
    builder.build_conditional_branch(is_neg, recip_bb, done_bb)?;

    builder.position_at_end(recip_bb);
    let recip = builder.build_int_signed_div(one, loop_result, &format!("{name}.recip"))?;
    builder.build_unconditional_branch(done_bb)?;

    builder.position_at_end(done_bb);
    let result_phi = builder.build_phi(i64_type, &format!("{name}.result"))?;
    result_phi.add_incoming(&[(&loop_result, loop_end_bb), (&recip, recip_bb)]);

    Ok(result_phi.as_basic_value().into_int_value())
}

// Lowers `dest = base ^ power` for a float base via the `llvm.powi`
// intrinsic, which natively supports negative integer exponents
// (`base^-n == 1/base^n`) like the evaluator's `f64::powi` -- no
// hand-rolled loop needed. Always targets the `llvm.powi.<T>.i32`
// overload, the one exponent width `llvm.powi` has always supported;
// no real exponent needs more than 32 bits anyway.
fn build_pow_intrinsic<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    base: FloatValue<'ctx>,
    power: IntValue<'ctx>,
    name: &str,
) -> Result<FloatValue<'ctx>, Box<dyn Error>> {
    let i32_type = context.i32_type();
    let power_i32 = builder.build_int_truncate(power, i32_type, &format!("{name}.power_i32"))?;

    let powi = Intrinsic::find("llvm.powi").expect("llvm.powi is a standard LLVM intrinsic");
    let decl = powi
        .get_declaration(module, &[base.get_type().into(), i32_type.into()])
        .expect("llvm.powi.<float>.i32 is a valid overload");

    let call = builder.build_call(decl, &[base.into(), power_i32.into()], name)?;
    Ok(call
        .try_as_basic_value()
        .basic()
        .expect("llvm.powi returns a float")
        .into_float_value())
}

fn compile_function<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    func: &BLIRFunction,
    printf: &Printf<'ctx>,
    string_globals: &HashMap<String, PointerValue<'ctx>>,
) -> Result<(), Box<dyn Error>> {
    let i64_type = context.i64_type();
    let f64_type = context.f64_type();
    let ptr_type = context.ptr_type(AddressSpace::default());

    // Function was pre-declared in convert_blir_to_llir_module; just retrieve it.
    let fn_val = module
        .get_function(&func.m_name)
        .expect("function should have been pre-declared");

    // Seed val_map with named function parameters.
    let mut val_map: HashMap<String, BasicValueEnum<'ctx>> = HashMap::new();
    for (i, BLIRRegister(name, _)) in func.m_params.iter().enumerate() {
        let param = fn_val.get_nth_param(i as u32).unwrap();
        param.set_name(name);
        val_map.insert(name.clone(), param);
    }

    let entry_block = context.append_basic_block(fn_val, "entry");
    builder.position_at_end(entry_block);

    // Pre-create a BasicBlock for every MarkLabel in the body so forward references work.
    let mut block_map: HashMap<String, BasicBlock<'ctx>> = HashMap::new();
    for op in &func.m_body {
        if let BLIROperation::MarkLabel(BLIRLabel(name)) = op
            && !block_map.contains_key(name)
        {
            block_map.insert(name.clone(), context.append_basic_block(fn_val, name));
        }
    }

    let mut alloca_map: HashMap<String, PointerValue<'ctx>> = HashMap::new();

    for op in &func.m_body {
        match op {
            BLIROperation::StackAllocVariable(BLIRRegister(name, ty)) => {
                let alloca = match ty {
                    BLIRType::Int64 => builder.build_alloca(i64_type, name)?,
                    BLIRType::Float64 => builder.build_alloca(f64_type, name)?,
                    _ => builder.build_alloca(ptr_type, name)?,
                };
                alloca_map.insert(name.clone(), alloca);
            }
            BLIROperation::StoreFromConst(constant, BLIRRegister(dest, _)) => {
                let ptr = alloca_map[dest];
                match constant {
                    BLIRConstant::Int64(v) => {
                        builder.build_store(ptr, i64_type.const_int(*v as u64, true))?;
                    }
                    BLIRConstant::Float64(v) => {
                        builder.build_store(ptr, f64_type.const_float(*v))?;
                    }
                    BLIRConstant::String(s) => {
                        builder.build_store(ptr, string_globals[s])?;
                    }
                }
            }
            BLIROperation::StoreFromReg(BLIRRegister(src, _), BLIRRegister(dest, _)) => {
                let dst = alloca_map[dest];
                let src_val = val_map[src];
                builder.build_store(dst, src_val)?;
            }
            BLIROperation::Load(BLIRRegister(from, from_ty), BLIRRegister(into, _)) => {
                if let Some(&ptr) = alloca_map.get(from) {
                    let loaded = match from_ty {
                        BLIRType::Int64 => builder.build_load(i64_type, ptr, into)?,
                        BLIRType::Float64 => builder.build_load(f64_type, ptr, into)?,
                        _ => builder.build_load(ptr_type, ptr, into)?,
                    };
                    val_map.insert(into.clone(), loaded);
                } else if let Some(&val) = val_map.get(from) {
                    // Function parameter — already an SSA value; alias without emitting a load.
                    val_map.insert(into.clone(), val);
                }
            }
            BLIROperation::CallPrintf(BLIRRegister(name, ty)) => {
                let val = val_map[name];
                match ty {
                    // Numbers have no in-memory text form, so print them via
                    // a %lld/%f format string; strings already are the text
                    // to print (with a trailing newline baked into their
                    // global), so they're passed as printf's format directly.
                    BLIRType::Int64 => {
                        builder.build_call(
                            printf.func,
                            &[printf.int_fmt.into(), val.into()],
                            "printf_ret",
                        )?;
                    }
                    BLIRType::Float64 => {
                        builder.build_call(
                            printf.func,
                            &[printf.float_fmt.into(), val.into()],
                            "printf_ret",
                        )?;
                    }
                    _ => {
                        builder.build_call(printf.func, &[val.into()], "printf_ret")?;
                    }
                };
            }
            BLIROperation::ICmp(
                cmp_op,
                BLIRRegister(dest, _),
                BLIRRegister(lhs, lhs_ty),
                BLIRRegister(rhs, _),
            ) => {
                // The compared operands (not the boolean-typed `dest`) decide
                // whether this is an integer or float comparison.
                let result = if *lhs_ty == BLIRType::Float64 {
                    let lhs_val = val_map[lhs].into_float_value();
                    let rhs_val = val_map[rhs].into_float_value();
                    builder.build_float_compare(
                        ir_cmp_to_float_predicate(cmp_op),
                        lhs_val,
                        rhs_val,
                        dest,
                    )?
                } else {
                    let lhs_val = val_map[lhs].into_int_value();
                    let rhs_val = val_map[rhs].into_int_value();
                    builder.build_int_compare(
                        ir_cmp_to_predicate(cmp_op),
                        lhs_val,
                        rhs_val,
                        dest,
                    )?
                };
                val_map.insert(dest.clone(), result.into());
            }
            BLIROperation::CondBranch(
                BLIRRegister(cond, _),
                BLIRLabel(false_lbl),
                BLIRLabel(true_lbl),
            ) => {
                let cond_val = val_map[cond].into_int_value();
                builder.build_conditional_branch(
                    cond_val,
                    block_map[true_lbl],
                    block_map[false_lbl],
                )?;
            }
            BLIROperation::MarkLabel(BLIRLabel(name)) => {
                let next_block = block_map[name];
                // Auto-terminate the current block before switching to the new one.
                if builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    builder.build_unconditional_branch(next_block)?;
                }
                builder.position_at_end(next_block);
            }
            BLIROperation::JumpToLabel(BLIRLabel(name)) => {
                builder.build_unconditional_branch(block_map[name])?;
            }
            BLIROperation::IArith(
                arith_op,
                BLIRRegister(dest, dest_ty),
                BLIRRegister(lhs, _),
                BLIRRegister(rhs, _),
            ) => {
                let result: BasicValueEnum = if *dest_ty == BLIRType::Float64 {
                    let lhs_val = val_map[lhs].into_float_value();
                    let rhs_val = val_map[rhs].into_float_value();
                    match arith_op {
                        BLIRArithOp::Add => builder.build_float_add(lhs_val, rhs_val, dest)?.into(),
                        BLIRArithOp::Sub => builder.build_float_sub(lhs_val, rhs_val, dest)?.into(),
                        BLIRArithOp::Mul => builder.build_float_mul(lhs_val, rhs_val, dest)?.into(),
                        BLIRArithOp::Div => builder.build_float_div(lhs_val, rhs_val, dest)?.into(),
                    }
                } else {
                    let lhs_val = val_map[lhs].into_int_value();
                    let rhs_val = val_map[rhs].into_int_value();
                    match arith_op {
                        BLIRArithOp::Add => builder.build_int_add(lhs_val, rhs_val, dest)?.into(),
                        BLIRArithOp::Sub => builder.build_int_sub(lhs_val, rhs_val, dest)?.into(),
                        BLIRArithOp::Mul => builder.build_int_mul(lhs_val, rhs_val, dest)?.into(),
                        BLIRArithOp::Div => {
                            builder.build_int_signed_div(lhs_val, rhs_val, dest)?.into()
                        }
                    }
                };
                val_map.insert(dest.clone(), result);
            }
            BLIROperation::LoadConst(constant, BLIRRegister(dest, _)) => {
                let val: BasicValueEnum = match constant {
                    BLIRConstant::Int64(v) => i64_type.const_int(*v as u64, true).into(),
                    BLIRConstant::Float64(v) => f64_type.const_float(*v).into(),
                    BLIRConstant::String(s) => string_globals[s].into(),
                };
                val_map.insert(dest.clone(), val);
            }
            BLIROperation::CallFunction(fn_name, args, BLIRRegister(dest, _)) => {
                let callee = module
                    .get_function(fn_name)
                    .ok_or_else(|| LLIRGenError::UnsupportedCallTarget(fn_name.clone()))?;
                let arg_vals: Vec<_> = args
                    .iter()
                    .map(|BLIRRegister(name, _)| val_map[name].into())
                    .collect();
                let call = builder.build_call(callee, &arg_vals, dest)?;
                if let Some(val) = call.try_as_basic_value().basic() {
                    val_map.insert(dest.clone(), val);
                }
            }
            BLIROperation::PowII(
                BLIRRegister(dest, _),
                BLIRRegister(base, _),
                BLIRRegister(power, _),
            ) => {
                let base_val = val_map[base].into_int_value();
                let power_val = val_map[power].into_int_value();
                let result = build_int_pow(context, builder, fn_val, base_val, power_val, dest)?;
                val_map.insert(dest.clone(), result.into());
            }
            BLIROperation::PowFI(
                BLIRRegister(dest, _),
                BLIRRegister(base, _),
                BLIRRegister(power, _),
            ) => {
                let base_val = val_map[base].into_float_value();
                let power_val = val_map[power].into_int_value();
                let result =
                    build_pow_intrinsic(context, module, builder, base_val, power_val, dest)?;
                val_map.insert(dest.clone(), result.into());
            }
            BLIROperation::PowFF(
                BLIRRegister(dest, _),
                BLIRRegister(base, _),
                BLIRRegister(power, _),
            ) => {
                let base_val = val_map[base].into_float_value();
                let power_float = val_map[power].into_float_value();
                let power_val = builder.build_float_to_signed_int(
                    power_float,
                    i64_type,
                    &format!("{dest}.pow_power_int"),
                )?;
                let result =
                    build_pow_intrinsic(context, module, builder, base_val, power_val, dest)?;
                val_map.insert(dest.clone(), result.into());
            }
        }
    }

    // Auto-return if the final block has no terminator.
    if builder
        .get_insert_block()
        .unwrap()
        .get_terminator()
        .is_none()
    {
        match func.m_return {
            BLIRType::Int64 => {
                let ret_val = func
                    .m_return_reg
                    .as_ref()
                    .and_then(|r| val_map.get(r))
                    .map(|v| v.into_int_value())
                    .ok_or_else(|| LLIRGenError::MissingReturnRegister(func.m_name.clone()))?;
                builder.build_return(Some(&ret_val))?;
            }
            BLIRType::Float64 => {
                let ret_val = func
                    .m_return_reg
                    .as_ref()
                    .and_then(|r| val_map.get(r))
                    .map(|v| v.into_float_value())
                    .ok_or_else(|| LLIRGenError::MissingReturnRegister(func.m_name.clone()))?;
                builder.build_return(Some(&ret_val))?;
            }
            _ => {
                builder.build_return(None)?;
            }
        }
    }

    Ok(())
}

pub(super) fn convert_blir_to_llir_module<'ctx>(
    context: &'ctx Context,
    blir_program: &BLIRCompileUnit,
) -> Result<Module<'ctx>, Box<dyn Error>> {
    let module: Module = context.create_module(blir_program.m_name.as_str());
    let builder = context.create_builder();

    let i32_type = context.i32_type();
    let i64_type = context.i64_type();
    let f64_type = context.f64_type();
    let ptr_type = context.ptr_type(AddressSpace::default());
    let void_type = context.void_type();

    // Declare printf, plus the format strings `print` uses to render
    // numeric values (a printed string prints its own bytes directly instead).
    let printf = Printf {
        func: module.add_function("printf", i32_type.fn_type(&[ptr_type.into()], true), None),
        int_fmt: add_c_string_global(context, &module, "%lld\n", "fmt.int"),
        float_fmt: add_c_string_global(context, &module, "%f\n", "fmt.float"),
    };

    // Emit a private unnamed_addr global for each string constant.
    let mut string_globals: HashMap<String, PointerValue<'_>> = HashMap::new();
    for s in &blir_program.m_string_consts {
        let ptr = add_c_string_global(context, &module, &format!("{}\n", s), &str_global_name(s));
        string_globals.insert(s.clone(), ptr);
    }

    // Pre-declare all functions so calls work regardless of definition order.
    for func in &blir_program.m_functions {
        let param_types: Vec<_> = func
            .m_params
            .iter()
            .map(|BLIRRegister(_, ty)| match ty {
                BLIRType::Int64 => i64_type.into(),
                BLIRType::Float64 => f64_type.into(),
                _ => ptr_type.into(),
            })
            .collect();
        match func.m_return {
            BLIRType::Int64 => {
                module.add_function(&func.m_name, i64_type.fn_type(&param_types, false), None)
            }
            BLIRType::Float64 => {
                module.add_function(&func.m_name, f64_type.fn_type(&param_types, false), None)
            }
            _ => module.add_function(&func.m_name, void_type.fn_type(&param_types, false), None),
        };
    }

    for func in &blir_program.m_functions {
        compile_function(context, &module, &builder, func, &printf, &string_globals)?;
    }

    Ok(module)
}
