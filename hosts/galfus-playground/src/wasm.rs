use std::fmt;

use crate::Playground;
use js_sys::Function;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = Playground)]
pub struct WasmPlayground {
    playground: Playground,
}

#[wasm_bindgen(js_class = Playground)]
impl WasmPlayground {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            playground: Playground::new(),
        }
    }

    #[wasm_bindgen(js_name = setWriteCallback)]
    pub fn set_write_callback(&self, callback: Function) {
        self.playground.set_write_callback(callback);
    }

    #[wasm_bindgen(js_name = sendReadData)]
    pub fn send_read_data(&self, bytes: &[u8]) {
        self.playground.send_read_data(bytes);
    }

    #[wasm_bindgen(js_name = setConfig)]
    pub fn set_config(&mut self, config: &str) -> String {
        match self.playground.set_config(config.as_bytes()) {
            Ok(()) => success_json(),
            Err(error) => error_json(error),
        }
    }

    #[wasm_bindgen(js_name = setSource)]
    pub fn set_source(&mut self, path: &str, source: &str) -> String {
        match self.playground.set_source(path, source.as_bytes()) {
            Ok(()) => success_json(),
            Err(error) => error_json(error),
        }
    }

    #[wasm_bindgen(js_name = check)]
    pub fn check(&mut self) -> String {
        let result = self.playground.check();
        serde_json::json!({
            "is_valid": result.is_valid,
            "diagnostics": result.diagnostics,
        })
        .to_string()
    }

    #[wasm_bindgen(js_name = compile)]
    pub fn compile(&mut self) -> String {
        match self.playground.compile() {
            Ok(()) => success_json(),
            Err(error) => error_json(error),
        }
    }

    #[wasm_bindgen(js_name = start)]
    pub fn start(&mut self, args_json: &str) -> String {
        let args = match serde_json::from_str::<Vec<String>>(args_json) {
            Ok(args) => args.into_iter().map(String::into_bytes).collect::<Vec<_>>(),
            Err(error) => return error_json(error),
        };
        match self.playground.start(args.as_slice()) {
            Ok(()) => success_json(),
            Err(error) => error_json(error),
        }
    }

    #[wasm_bindgen(js_name = step)]
    pub fn step(&mut self) -> String {
        match self.playground.step() {
            Ok(galfus_contract::ExecutorStepResult::Running) => serde_json::json!({
                "status": "running",
                "output": String::from_utf8_lossy(self.playground.take_output().as_slice()),
                "error": null,
            })
            .to_string(),
            Ok(galfus_contract::ExecutorStepResult::Blocked { .. }) => serde_json::json!({
                "status": "pending_read",
                "output": String::from_utf8_lossy(self.playground.take_output().as_slice()),
                "error": null,
            })
            .to_string(),
            Ok(galfus_contract::ExecutorStepResult::Completed(exit_code)) => serde_json::json!({
                "status": "completed",
                "exit_code": exit_code,
                "output": String::from_utf8_lossy(self.playground.take_output().as_slice()),
                "error": null,
            })
            .to_string(),
            Err(error) => error_json(error),
        }
    }

    #[wasm_bindgen(js_name = run)]
    pub fn run(&mut self, args_json: &str) -> String {
        let args = match serde_json::from_str::<Vec<String>>(args_json) {
            Ok(args) => args.into_iter().map(String::into_bytes).collect::<Vec<_>>(),
            Err(error) => return error_json(error),
        };
        match self.playground.run(args.as_slice()) {
            Ok(exit_code) => serde_json::json!({
                "exit_code": exit_code,
                "output": String::from_utf8_lossy(self.playground.take_output().as_slice()),
                "error": null,
            })
            .to_string(),
            Err(error) => error_json(error),
        }
    }

    #[wasm_bindgen(js_name = getVersion)]
    pub fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

fn success_json() -> String {
    serde_json::json!({ "ok": true }).to_string()
}

fn error_json(error: impl fmt::Display) -> String {
    serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
}
