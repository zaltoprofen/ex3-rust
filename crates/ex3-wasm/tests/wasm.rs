#![cfg(target_arch = "wasm32")]

use ex3_wasm::Ex3Session;
use js_sys::Reflect;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn compile_step_and_run_are_callable_through_the_wasm_boundary() {
    let mut session = Ex3Session::new();
    let compiled = session
        .compile_and_load("int main(void) { return 42; }")
        .unwrap();
    assert!(Reflect::get(&compiled, &JsValue::from_str("assembly"))
        .unwrap()
        .is_string());
    let source_map = Reflect::get(&compiled, &JsValue::from_str("sourceMap")).unwrap();
    assert!(js_sys::Array::is_array(&source_map));
    assert!(js_sys::Array::from(&source_map).length() > 0);
    let snapshot = session.snapshot().unwrap();
    assert!(
        Reflect::get(&snapshot, &JsValue::from_str("executedInstructions"))
            .unwrap()
            .as_f64()
            .is_some()
    );
    assert!(session.memory_range(0xffff, 8).is_ok());
    assert!(session.disassembly_range(0x10, 16).is_ok());
    assert!(session.toggle_breakpoint(0x10));
    assert!(session.breakpoints().is_ok());
    assert!(session.run_chunk(10).is_ok());
    session.clear_breakpoints();
    assert!(session.step().is_ok());
    assert!(session.reset().is_ok());
    assert_eq!(session.serial_output(), "");
    assert!(session.run_chunk(1_000_000).is_ok());

    let error = session.compile_and_load("int main( {").unwrap_err();
    assert_eq!(
        Reflect::get(&error, &JsValue::from_str("stage"))
            .unwrap()
            .as_string()
            .as_deref(),
        Some("compiler")
    );
}
