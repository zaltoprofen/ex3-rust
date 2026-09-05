pub mod dto;
pub mod error;
pub mod session;

use crate::{error::Ex3Error, session::SessionCore};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Ex3Session {
    core: SessionCore,
}

impl Default for Ex3Session {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Ex3Session {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            core: SessionCore::new(),
        }
    }

    pub fn compile_and_load(&mut self, source: &str) -> Result<JsValue, JsValue> {
        serialize_result(self.core.compile_and_load(source))
    }

    pub fn reset(&mut self) -> Result<JsValue, JsValue> {
        serialize_result(self.core.reset())
    }

    pub fn step(&mut self) -> Result<JsValue, JsValue> {
        serialize_result(self.core.step())
    }

    pub fn run_chunk(&mut self, max_instructions: u32) -> Result<JsValue, JsValue> {
        serialize_result(self.core.run_chunk(max_instructions))
    }

    pub fn snapshot(&self) -> Result<JsValue, JsValue> {
        serialize_result(self.core.snapshot())
    }

    pub fn memory_range(&self, start: u16, count: u32) -> Result<JsValue, JsValue> {
        serialize_result(self.core.memory_range(start, count))
    }

    pub fn disassembly_range(&self, start: u16, count: u32) -> Result<JsValue, JsValue> {
        serialize_result(self.core.disassembly_range(start, count))
    }

    pub fn toggle_breakpoint(&mut self, address: u16) -> bool {
        self.core.toggle_breakpoint(address)
    }

    pub fn clear_breakpoints(&mut self) {
        self.core.clear_breakpoints();
    }

    pub fn breakpoints(&self) -> Result<JsValue, JsValue> {
        to_js_value(&self.core.breakpoints())
    }

    pub fn serial_output(&self) -> String {
        self.core.serial_output()
    }
}

fn serialize_result<T: Serialize>(result: Result<T, Ex3Error>) -> Result<JsValue, JsValue> {
    match result {
        Ok(value) => to_js_value(&value),
        Err(error) => {
            Err(to_js_value(&error).unwrap_or_else(|_| JsValue::from_str(&error.message)))
        }
    }
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| {
        let error = Ex3Error::session(format!("failed to serialize WASM response: {error}"));
        serde_wasm_bindgen::to_value(&error).unwrap_or_else(|_| JsValue::from_str(&error.message))
    })
}
