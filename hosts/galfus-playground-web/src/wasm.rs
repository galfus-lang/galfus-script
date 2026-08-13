use std::fmt;

use crate::Playground;
use galfus_bytecode::PackageMetadata;
use galfus_contract::Providers;
use galfus_runtime::Execution;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// Estado global para controlar a execução atual. Permite abortar a VM se `start` for chamado novamente.
thread_local! {
    static CURRENT_EXECUTION: RefCell<Option<Rc<RefCell<Execution>>>> = RefCell::new(None);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = setTimeout)]
    fn set_timeout(callback: &js_sys::Function, delay: i32);
}

/// Força a Wasm a ceder controle para o Event Loop (macrotask),
/// permitindo que Promises de I/O resolvam e a UI não congele.
async fn yield_macro_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        set_timeout(&resolve, 0);
    });
    let _ = JsFuture::from(promise).await;
}

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
    pub async fn start(&mut self, options: js_sys::Object) -> Result<JsValue, JsValue> {
        // Aborta qualquer execução anterior que ainda esteja rodando
        CURRENT_EXECUTION.with(|exec| {
            let mut exec_opt = exec.borrow_mut();
            if let Some(existing) = exec_opt.take() {
                existing.borrow_mut().shutdown();
            }
        });

        // 1. Extrair 'args'
        let mut args: Vec<Vec<u8>> = Vec::new();
        if let Ok(args_val) = js_sys::Reflect::get(&options, &JsValue::from_str("args")) {
            if !args_val.is_undefined() && !args_val.is_null() {
                let js_args = js_sys::Array::from(&args_val);
                for i in 0..js_args.length() {
                    if let Some(s) = js_args.get(i).as_string() {
                        args.push(s.into_bytes());
                    }
                }
            }
        }

        // 2. Extrair 'envs'
        let mut env_vars = std::collections::HashMap::new();
        if let Ok(env_val) = js_sys::Reflect::get(&options, &JsValue::from_str("envs")) {
            if !env_val.is_undefined() && !env_val.is_null() {
                if let Ok(keys) = js_sys::Reflect::own_keys(&env_val) {
                    for i in 0..keys.length() {
                        let key = keys.get(i);
                        if let Ok(value) = js_sys::Reflect::get(&env_val, &key) {
                            if let (Some(k), Some(v)) = (key.as_string(), value.as_string()) {
                                env_vars.insert(k, v);
                            }
                        }
                    }
                }
            }
        }

        // 3. Extrair 'stdin' e 'stdout'
        let mut stdin_stream = None;
        if let Ok(stdin_val) = js_sys::Reflect::get(&options, &JsValue::from_str("stdin")) {
            if !stdin_val.is_undefined() && !stdin_val.is_null() {
                stdin_stream = Some(web_sys::ReadableStream::from(stdin_val));
            }
        }

        let mut stdout_stream = None;
        if let Ok(stdout_val) = js_sys::Reflect::get(&options, &JsValue::from_str("stdout")) {
            if !stdout_val.is_undefined() && !stdout_val.is_null() {
                stdout_stream = Some(web_sys::WritableStream::from(stdout_val));
            }
        }

        // Inicializar os Providers usando o galfus-host-web
        let metadata = PackageMetadata {
            name: "playground".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            description: None,
        };

        let providers = Providers::new()
            .with_host(
                "env",
                Box::new(galfus_host_web::providers::env::WebEnvProvider::new(
                    metadata, env_vars,
                )),
            )
            .with_host(
                "io",
                Box::new(galfus_host_web::providers::io::WebIoProvider::new(
                    stdin_stream,
                    stdout_stream,
                )),
            )
            .with_host(
                "time",
                Box::new(galfus_host_web::providers::time::WebTimeProvider::new()),
            );

        // Inicializar o Driver Kernel Single-Thread
        let driver = Rc::new(galfus_host_web::driver::single::WebKernelDriver::new());

        // Iniciar execução no Workspace
        let workspace = self.playground.get_workspace();
        let execution = workspace
            .start_execution(&args, Some(providers), driver)
            .map_err(|e| JsValue::from_str(&format!("Initialization failure: {:?}", e)))?;

        let exec_rc = Rc::new(RefCell::new(execution));

        CURRENT_EXECUTION.with(|exec| {
            *exec.borrow_mut() = Some(exec_rc.clone());
        });

        // Loop cooperativo de polling assíncrono
        loop {
            let state = {
                let mut exec = exec_rc.borrow_mut();
                exec.poll(100)
                    .map_err(|e| JsValue::from_str(&format!("Execution failure: {:?}", e)))?
            };

            match state {
                galfus_contract::ExecutorStepResult::Completed(_) => {
                    let result = {
                        let mut exec = exec_rc.borrow_mut();
                        exec.run_sync_to_completion().map_err(|e| {
                            JsValue::from_str(&format!("Completion failure: {:?}", e))
                        })?
                    };

                    // Limpar do estado global
                    CURRENT_EXECUTION.with(|exec| {
                        let mut exec_opt = exec.borrow_mut();
                        if let Some(e) = exec_opt.as_ref() {
                            if Rc::ptr_eq(e, &exec_rc) {
                                *exec_opt = None;
                            }
                        }
                    });

                    // Converter o BoundaryValue (i32) para JsValue
                    if let galfus_contract::BoundaryValue::I32(code) = result {
                        return Ok(JsValue::from(code));
                    } else {
                        return Ok(JsValue::from(0));
                    }
                }
                galfus_contract::ExecutorStepResult::Blocked { .. } => {
                    yield_macro_task().await;
                }
                galfus_contract::ExecutorStepResult::Running => {
                    yield_macro_task().await;
                }
            }
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
