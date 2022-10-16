mod func;

use std::path::Path;

use anyhow::Result;
use wasmtime::*;
use wasmtime_wasi::{sync::WasiCtxBuilder, WasiCtx};

struct SomeData {
    wasi: WasiCtx,
}

fn benchmark<T>(mut func: impl FnMut() -> T, times: usize) -> (std::time::Duration, Vec<T>) {
    let mut buf = Vec::with_capacity(times);

    let start = std::time::Instant::now();
    for _ in 0..times {
        buf.push(func());
    }
    let ret = start.elapsed();

    (ret, buf)
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

fn main() -> Result<()> {
    // let engine = Engine::new(&Config::new().consume_fuel(true))?;
    let engine = Engine::new(&Config::new().consume_fuel(false))?;

    let module = load_module(&engine, "target/wasm32-wasi/debug/wasm-test.wasm")?;
    let module2 = tekito_wasi_module(&engine)?;
    let module3 = load_module(
        &engine,
        "../rust-wasm-test-lib/target/wasm32-unknown-unknown/release/rust-wasm-test-lib.wasm",
    )?;

    for _ in 0..10 {
        let (linker, mut store) = {
            let start = std::time::Instant::now();

            // Define the WASI functions globally on the `Config`.
            // let engine = Engine::default();
            let mut linker = Linker::new(&engine);
            wasmtime_wasi::add_to_linker(&mut linker, |s: &mut SomeData| &mut s.wasi)?;

            // Create a WASI context and put it in a Store; all instances in the store
            // share this context. `WasiCtxBuilder` provides a number of ways to
            // configure what the target program will have access to.
            let wasi = WasiCtxBuilder::new()
                .inherit_stdio()
                // .inherit_args()?
                .build();
            let mut store = Store::new(&engine, SomeData { wasi });
            // store.add_fuel(u64::MAX).unwrap();
            linker.module(&mut store, "", &module)?;
            linker.module(&mut store, "", &module2)?;
            linker.module(&mut store, "", &module3)?;

            let elapsed = start.elapsed();
            println!("Linker created in {:?}", elapsed);

            (linker, store)
        };

        let wasi_default_func = linker
            .get_default(&mut store, "")?
            .typed::<(), (), _>(&store)?;

        wasi_default_func.call(&mut store, ())?;

        let func = linker
            .get(&mut store, "", "gcd")
            .unwrap()
            .into_func()
            .unwrap()
            .typed::<(i32, i32), i32, _>(&store)?;
        let read_a = linker
            .get(&mut store, "", "a")
            .unwrap()
            .into_func()
            .unwrap()
            .typed::<(), i32, _>(&store)?;
        let read_b = linker
            .get(&mut store, "", "b")
            .unwrap()
            .into_func()
            .unwrap()
            .typed::<(), i32, _>(&store)?;
        let read_c = linker
            .get(&mut store, "", "c")
            .unwrap()
            .into_func()
            .unwrap()
            .typed::<(), u64, _>(&store)?;
        let is_prime_wasm = linker
            .get(&mut store, "", "is_prime")
            .unwrap()
            .into_func()
            .unwrap()
            .typed::<(u64,), i32, _>(&store)?;
        let a = read_a.call(&mut store, ())?;
        let b = read_b.call(&mut store, ())?;
        let c = read_c.call(&mut store, ())?;

        let (elapsed, buff) = benchmark(|| read_a.call(&mut store, ()).unwrap(), 10000000);
        println!("read_a() = {}, time = {:?}", buff[0], elapsed);

        let (elapsed, buff) = benchmark(|| func.call(&mut store, (a, b)).unwrap(), 10000000);
        println!("gcd_wasm({}, {}) = {}, time = {:?}", a, b, buff[0], elapsed);

        let (elapsed, buff) = benchmark(|| func::gcd(a, b), 10000000);
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

        let (elapsed, buff) = benchmark(|| func::is_prime(c), 10000000);
        println!("is_prime_rust({}) = {:?}, time = {:?}", c, buff[0], elapsed);

        let (elapsed, buff) = benchmark(|| func::is_prime(c + 1), 10000000);
        println!(
            "is_prime_rust({}) = {:?}, time = {:?}",
            c + 1,
            buff[0],
            elapsed
        );
    }

    Ok(())
}
