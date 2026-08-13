pub mod driver;
pub mod providers;

use galfus_bytecode::PackageImage;
use galfus_contract::{AdapterBindings, ExecutionFailure, Providers};
use galfus_runtime::driver::ExecutionDriver;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

// No PackageLoader needed anymore.

use galfus_contract::RuntimeCapabilities;
use galfus_runtime::Runtime;

pub struct ExecutionHost {
    providers: Providers,
    adapters: AdapterBindings,
    driver: Rc<dyn ExecutionDriver>,
}

impl ExecutionHost {
    pub fn new(
        providers: Providers,
        adapters: AdapterBindings,
        driver: Rc<dyn ExecutionDriver>,
    ) -> Self {
        Self {
            providers,
            adapters,
            driver,
        }
    }

    pub fn run(
        self,
        package: std::sync::Arc<PackageImage>,
        args: &[Vec<u8>],
    ) -> Result<i32, ExecutionFailure> {
        let capabilities = RuntimeCapabilities::builder()
            .with_providers(self.providers)
            .with_adapter_bindings(self.adapters)
            .build();

        let runtime = Runtime::new(package, capabilities);

        let mut execution = runtime.start(args, self.driver).map_err(|e| {
            ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InitializationFailure,
                e.to_string(),
            )
        })?;

        let result = execution.run_sync_to_completion().map_err(|e| {
            ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InternalRuntimeFailure,
                e.to_string(),
            )
        })?;

        if let galfus_contract::BoundaryValue::I32(code) = result {
            Ok(code)
        } else {
            Ok(0)
        }
    }
}

/// Ponto de entrada exportado para o Javascript.
/// Simula a execução nativa, aguardando (bloqueando ou rodando em loop) a execução até o fim.
#[wasm_bindgen]
pub fn start(options: js_sys::Object) -> Result<i32, JsValue> {
    // 1. Extrair o 'blob'
    let blob_val = js_sys::Reflect::get(&options, &JsValue::from_str("blob"))
        .map_err(|_| JsValue::from_str("Missing 'blob' property in options"))?;

    if blob_val.is_undefined() || blob_val.is_null() {
        return Err(JsValue::from_str("'blob' property is required"));
    }

    let uint8_arr = js_sys::Uint8Array::new(&blob_val);
    let bytecode = uint8_arr.to_vec();

    let package =
        PackageImage::from_bytecode(&bytecode).map_err(|e| JsValue::from_str(&e.to_string()))?;

    // 2. Extrair 'args'
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

    // 3. Extrair 'envs'
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

    // 4. Extrair 'stdin' e 'stdout'
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

    // A integração real dos Providers virá na etapa seguinte.
    let providers = Providers::new()
        .with_host(
            "env",
            Box::new(providers::env::WebEnvProvider::new(
                package.metadata().clone(),
                env_vars,
            )),
        )
        .with_host(
            "io",
            Box::new(providers::io::WebIoProvider::new(
                stdin_stream,
                stdout_stream,
            )),
        )
        .with_host("time", Box::new(providers::time::WebTimeProvider::new()));
    let adapters = AdapterBindings::default();

    #[cfg(feature = "multi_thread")]
    let driver = Rc::new(driver::pool::WebWorkersDriver::new());

    #[cfg(not(feature = "multi_thread"))]
    let driver = Rc::new(driver::single::WebKernelDriver::new());

    let host = ExecutionHost::new(providers, adapters, driver);

    match host.run(std::sync::Arc::new(package), &args) {
        Ok(code) => Ok(code),
        Err(e) => Err(JsValue::from_str(&format!("Execution failed: {:?}", e))),
    }
}
