#![allow(clippy::result_large_err)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod driver;
pub mod providers;

use galfus_bytecode::PackageImage;
use galfus_contract::{AdapterBindings, ExecutionFailure, Providers};
use galfus_runtime::driver::ExecutionDriver;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

// No PackageLoader needed anymore.

use galfus_contract::RuntimeCapabilities;
use galfus_runtime::Execution;
use galfus_runtime::Runtime;
use std::cell::RefCell;
use wasm_bindgen_futures::JsFuture;

thread_local! {
    static CURRENT_EXECUTION: RefCell<Option<Rc<RefCell<Execution>>>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = setTimeout)]
    fn set_timeout(callback: &js_sys::Function, delay: i32);
}

async fn yield_macro_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        set_timeout(&resolve, 0);
    });
    let _ = JsFuture::from(promise).await;
}

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

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export interface GalfusWebOptions {
    blob: Uint8Array;
    args?: string[];
    envs?: Record<string, string>;
    stdin?: any;
    stdout?: any;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "GalfusWebOptions")]
    pub type GalfusWebOptions;
}

#[wasm_bindgen]
pub async fn start(options: GalfusWebOptions) -> Result<JsValue, JsValue> {
    CURRENT_EXECUTION.with(|exec| {
        let mut exec_opt = exec.borrow_mut();
        if let Some(existing) = exec_opt.take() {
            existing.borrow_mut().shutdown();
        }
    });
    let blob_val = js_sys::Reflect::get(&options, &JsValue::from_str("blob"))
        .map_err(|_| JsValue::from_str("Missing 'blob' property in options"))?;

    if blob_val.is_undefined() || blob_val.is_null() {
        return Err(JsValue::from_str("'blob' property is required"));
    }

    let uint8_arr = js_sys::Uint8Array::new(&blob_val);
    let bytecode = uint8_arr.to_vec();

    let package =
        PackageImage::from_bytecode(&bytecode).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let mut args: Vec<Vec<u8>> = Vec::new();
    if let Ok(args_val) = js_sys::Reflect::get(&options, &JsValue::from_str("args"))
        && !args_val.is_undefined()
        && !args_val.is_null()
    {
        let js_args = js_sys::Array::from(&args_val);
        for i in 0..js_args.length() {
            if let Some(s) = js_args.get(i).as_string() {
                args.push(s.into_bytes());
            }
        }
    }

    let mut env_vars = std::collections::HashMap::new();
    if let Ok(env_val) = js_sys::Reflect::get(&options, &JsValue::from_str("envs"))
        && !env_val.is_undefined()
        && !env_val.is_null()
        && let Ok(keys) = js_sys::Reflect::own_keys(&env_val)
    {
        for i in 0..keys.length() {
            let key = keys.get(i);
            if let Ok(value) = js_sys::Reflect::get(&env_val, &key)
                && let (Some(k), Some(v)) = (key.as_string(), value.as_string())
            {
                env_vars.insert(k, v);
            }
        }
    }

    let mut stdin_stream = None;
    if let Ok(stdin_val) = js_sys::Reflect::get(&options, &JsValue::from_str("stdin"))
        && !stdin_val.is_undefined()
        && !stdin_val.is_null()
    {
        stdin_stream = Some(web_sys::ReadableStream::from(stdin_val));
    }

    let mut stdout_stream = None;
    if let Ok(stdout_val) = js_sys::Reflect::get(&options, &JsValue::from_str("stdout"))
        && !stdout_val.is_undefined()
        && !stdout_val.is_null()
    {
        stdout_stream = Some(web_sys::WritableStream::from(stdout_val));
    }

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

    let capabilities = RuntimeCapabilities::builder()
        .with_providers(host.providers)
        .with_adapter_bindings(host.adapters)
        .build();

    let runtime = Runtime::new(std::sync::Arc::new(package), capabilities);

    let execution = runtime
        .start(&args, host.driver)
        .map_err(|e| JsValue::from_str(&format!("Initialization failure: {:?}", e)))?;

    let exec_rc = Rc::new(RefCell::new(execution));

    CURRENT_EXECUTION.with(|exec| {
        *exec.borrow_mut() = Some(exec_rc.clone());
    });

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
                    exec.run_sync_to_completion()
                        .map_err(|e| JsValue::from_str(&format!("Completion failure: {:?}", e)))?
                };

                CURRENT_EXECUTION.with(|exec| {
                    let mut exec_opt = exec.borrow_mut();
                    if let Some(e) = exec_opt.as_ref()
                        && Rc::ptr_eq(e, &exec_rc)
                    {
                        *exec_opt = None;
                    }
                });

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
