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
    let startup_stack = session.stack_view().unwrap();
    assert_eq!(
        Reflect::get(&startup_stack, &JsValue::from_str("context"))
            .unwrap()
            .as_string()
            .as_deref(),
        Some("startup")
    );
    assert_eq!(
        Reflect::get(&startup_stack, &JsValue::from_str("available"))
            .unwrap()
            .as_bool(),
        Some(false)
    );
    assert!(js_sys::Array::is_array(
        &Reflect::get(&startup_stack, &JsValue::from_str("rawStack")).unwrap()
    ));
    session.step().unwrap();
    let c_stack = session.stack_view_with_depth(16).unwrap();
    assert_eq!(
        Reflect::get(&c_stack, &JsValue::from_str("context"))
            .unwrap()
            .as_string()
            .as_deref(),
        Some("c-function")
    );
    let frames =
        js_sys::Array::from(&Reflect::get(&c_stack, &JsValue::from_str("frames")).unwrap());
    assert!(frames.length() > 0);
    let first_frame = frames.get(0);
    assert!(Reflect::has(&first_frame, &JsValue::from_str("functionName")).unwrap());
    assert!(Reflect::has(&first_frame, &JsValue::from_str("currentSp")).unwrap());
    let slots =
        js_sys::Array::from(&Reflect::get(&first_frame, &JsValue::from_str("slots")).unwrap());
    assert_eq!(
        Reflect::get(&slots.get(0), &JsValue::from_str("kind"))
            .unwrap()
            .as_string()
            .as_deref(),
        Some("return-address")
    );
    assert_eq!(
        Reflect::get(&slots.get(0), &JsValue::from_str("state"))
            .unwrap()
            .as_string()
            .as_deref(),
        Some("control")
    );
    assert!(session.stack_view_with_depth(0).is_err());
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
