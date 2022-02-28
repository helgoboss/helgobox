use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mlua::Lua;
use realearn_expression::api::ExpressionEvaluator;
use realearn_expression::eel2_impl::Eel2ExpressionEvaluator;
use realearn_expression::fasteval_impl::FastevalExpressionEvaluator;
use realearn_expression::lua_impl::{
    FunctionalLuaExpressionEvaluator, LuaExpressionEvaluator, ParameterLuaExpressionEvaluator,
};

pub fn criterion_benchmark(c: &mut Criterion) {
    // Input
    let expression = "2 * x";
    let vars = |name: &str, args: &[f64]| match name {
        "x" => Some(5.0),
        _ => None,
    };
    // Setup
    let fasteval_evaluator = FastevalExpressionEvaluator::compile(expression).unwrap();
    let eel2_evaluator = Eel2ExpressionEvaluator::compile(expression).unwrap();
    let lua = Lua::new();
    lua.gc_stop();
    let lua_evaluator = LuaExpressionEvaluator::compile(&lua, expression).unwrap();
    let functional_lua_evaluator =
        FunctionalLuaExpressionEvaluator::compile(&lua, expression).unwrap();
    let parameter_lua_evaluator =
        ParameterLuaExpressionEvaluator::compile(&lua, expression).unwrap();
    // Benchmark
    // c.bench_function("evaluate_via_fasteval", |b| {
    //     b.iter(|| {
    //         fasteval_evaluator.evaluate(vars).unwrap();
    //     })
    // });
    // c.bench_function("evaluate_via_eel2", |b| {
    //     b.iter(|| {
    //         eel2_evaluator.evaluate(vars).unwrap();
    //     })
    // });
    c.bench_function("evaluate_via_parameter_lua", |b| {
        b.iter(|| {
            functional_lua_evaluator.evaluate(vars).unwrap();
        })
    });
    c.bench_function("evaluate_via_lua", |b| {
        b.iter(|| {
            lua_evaluator.evaluate(vars).unwrap();
        })
    });
    c.bench_function("evaluate_via_functional_lua", |b| {
        b.iter(|| {
            functional_lua_evaluator.evaluate(vars).unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
