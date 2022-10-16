mod func;

use std::path::Path;

use anyhow::Result;
use wasmtime::*;
use wasmtime_wasi::{sync::WasiCtxBuilder, WasiCtx};

struct SomeData {
    wasi: WasiCtx,
}

use func::*;

fn get_typed_function_from_linker<T, Params, Results, S>(
    linker: &Linker<T>,
    module_name: &str,
    name: &str,
    store: &mut S,
) -> Result<TypedFunc<Params, Results>>
where
    Params: WasmParams,
    Results: WasmResults,
    S: AsContextMut<Data = T>,
{
    let func = linker
        .get(store.as_context_mut(), module_name, name)
        .ok_or_else(|| anyhow::anyhow!("Could not find function"))?
        .into_func()
        .ok_or_else(|| anyhow::anyhow!("Could not convert to function"))?
        .typed::<Params, Results, _>(store.as_context())?;
    Ok(func)
}

fn tekito_wasi_module(engine: &Engine) -> Result<Module> {
    let module2 = {
        let start = std::time::Instant::now();
        let module2 = Module::new(
            &engine,
            r#"(module
        (func $gcd (param i32 i32) (result i32)
          (local i32)
          block  ;; label = @1
            block  ;; label = @2
              local.get 0
              br_if 0 (;@2;)
              local.get 1
              local.set 2
              br 1 (;@1;)
            end
            loop  ;; label = @2
              local.get 1
              local.get 0
              local.tee 2
              i32.rem_u
              local.set 0
              local.get 2
              local.set 1
              local.get 0
              br_if 0 (;@2;)
            end
          end
          local.get 2
        )
        (func $a (result i32)
          i32.const 42
        )
        (func $b (result i32)
          i32.const 56
        )
        (func $c (result i64)
          i64.const 101
        )
        (export "gcd" (func $gcd))
        (export "a" (func $a))
        (export "b" (func $b))
        (export "c" (func $c))
      )"#,
        )?;
        let end = std::time::Instant::now();
        println!("Module loaded in {:?}", end - start);
        module2
    };
    Ok(module2)
}

fn load_module(engine: &Engine, path: impl AsRef<Path>) -> Result<Module> {
    let path = path.as_ref();
    let start = std::time::Instant::now();
    let module = Module::from_file(&engine, path)?;
    let end = std::time::Instant::now();
    println!("Module {:?} loaded in {:?}", path, end - start);
    Ok(module)
}

fn link_modules<'a, 'b>(
    engine: &'a Engine,
    modules: impl IntoIterator<Item = &'b (impl AsRef<str> + 'b, &'b Module)> + 'b,
) -> Result<(Linker<SomeData>, Store<SomeData>)> {
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |s: &mut SomeData| &mut s.wasi)?;

    let wasi = WasiCtxBuilder::new().inherit_stdio().build();

    let mut store = Store::new(&engine, SomeData { wasi });
    for (module_name, module) in modules {
        let module_name = module_name.as_ref();
        println!("Linking module: {}", module_name);
        for import in module.imports() {
            println!(
                "Import: {}.{}: {:?}",
                import.module(),
                import.name(),
                import.ty()
            );
        }
        for export in module.exports() {
            println!(
                "Export: {}.{}: {:?}",
                module_name,
                export.name(),
                export.ty()
            );
        }
        linker.module(&mut store, module_name, module)?;
    }
    Ok((linker, store))
}

fn main() -> Result<()> {
    let engine = Engine::new(&Config::new().consume_fuel(false))?;

    let module = load_module(&engine, "target/wasm32-wasi/debug/wasm-test.wasm")?;
    let module2 = tekito_wasi_module(&engine)?;
    let module3 = load_module(
        &engine,
        "../rust-wasm-test-lib/target/wasm32-unknown-unknown/release/rust-wasm-test-lib.wasm",
    )?;

    for _ in 0..2 {
        let (linker, mut store) =
            link_modules(&engine, &[("", &module), ("", &module2), ("", &module3)])?;

        let wasi_default_func = linker
            .get_default(&mut store, "")?
            .typed::<(), (), _>(&store)?;

        wasi_default_func.call(&mut store, ())?;

        let func = get_typed_function_from_linker::<_, (i32, i32), i32, _>(
            &linker, "", "gcd", &mut store,
        )?;
        let read_a = get_typed_function_from_linker::<_, (), i32, _>(&linker, "", "a", &mut store)?;
        let read_b = get_typed_function_from_linker::<_, (), i32, _>(&linker, "", "b", &mut store)?;
        let read_c = get_typed_function_from_linker::<_, (), u64, _>(&linker, "", "c", &mut store)?;
        let is_prime_wasm = get_typed_function_from_linker::<_, (u64,), i32, _>(
            &linker, "", "is_prime", &mut store,
        )?;
        let a = read_a.call(&mut store, ())?;
        let b = read_b.call(&mut store, ())?;
        let c = read_c.call(&mut store, ())?;

        let (elapsed, buff) = benchmark(|| read_a.call(&mut store, ()).unwrap(), 10000000);
        println!("read_a() = {}, time = {:?}", buff[0], elapsed);

        let (elapsed, buff) = benchmark(|| func.call(&mut store, (a, b)).unwrap(), 10000000);
        println!("gcd_wasm({}, {}) = {}, time = {:?}", a, b, buff[0], elapsed);

        let (elapsed, buff) = benchmark(|| gcd(a, b), 10000000);
        println!("gcd_rust({}, {}) = {}, time = {:?}", a, b, buff[0], elapsed);

        let (elapsed, buff) = benchmark(|| is_prime_wasm.call(&mut store, (c,)).unwrap(), 10000000);
        println!("is_prime_wasm({}) = {:?}, time = {:?}", c, buff[0], elapsed);

        let (elapsed, buff) = benchmark(
            || is_prime_wasm.call(&mut store, (c + 1,)).unwrap(),
            10000000,
        );
        println!(
            "is_prime_wasm({}) = {:?}, time = {:?}",
            c + 1,
            buff[0],
            elapsed
        );

        let (elapsed, buff) = benchmark(|| is_prime(c), 10000000);
        println!("is_prime_rust({}) = {:?}, time = {:?}", c, buff[0], elapsed);

        let (elapsed, buff) = benchmark(|| is_prime(c + 1), 10000000);
        println!(
            "is_prime_rust({}) = {:?}, time = {:?}",
            c + 1,
            buff[0],
            elapsed
        );
    }

    Ok(())
}
