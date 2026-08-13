pub mod driver;
pub mod providers;

use galfus_bytecode::PackageImage;
use galfus_contract::{AdapterBindings, ExecutionFailure, Providers};
use galfus_runtime::driver::ExecutionDriver;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

const MAGIC_MARKER: &[u8; 8] = b"GLFS_PKG";

pub struct PackageLoader {
    // Web-specific loader state (e.g. tracking fetch requests for secondary wasm)
}

impl PackageLoader {
    pub fn new() -> Self {
        Self {}
    }

    /// Procura pelo pacote anexado no final de um slice de bytes (ex: o próprio .wasm carregado via JS).
    pub fn extract_appended(binary: &[u8]) -> Option<Vec<u8>> {
        let len = binary.len();
        if len >= 16 {
            let magic_start = len - 8;
            let size_start = len - 16;
            let magic_buf = &binary[magic_start..len];

            if magic_buf == MAGIC_MARKER {
                let mut size_arr = [0u8; 8];
                size_arr.copy_from_slice(&binary[size_start..magic_start]);
                let payload_size = u64::from_le_bytes(size_arr) as usize;

                if len >= 16 + payload_size {
                    let payload_start = len - 16 - payload_size;
                    let payload = &binary[payload_start..payload_start + payload_size];
                    return Some(payload.to_vec());
                }
            }
        }
        None
    }

    pub fn load_from_bytes(&self, bytes: &[u8]) -> Result<PackageImage, String> {
        // Se houver um pacote anexado aos bytes recebidos, extraia-o.
        // Caso contrário, tenta processar os bytes diretamente como bytecode Galfus puro.
        let package_bytes = Self::extract_appended(bytes).unwrap_or_else(|| bytes.to_vec());
        PackageImage::from_bytecode(&package_bytes).map_err(|e| e.to_string())
    }
}

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
pub fn start(bytecode: &[u8], js_args: js_sys::Array) -> Result<i32, JsValue> {
    let loader = PackageLoader::new();
    let package = loader
        .load_from_bytes(bytecode)
        .map_err(|e| JsValue::from_str(&e))?;

    let mut args: Vec<Vec<u8>> = Vec::new();
    for i in 0..js_args.length() {
        if let Some(s) = js_args.get(i).as_string() {
            args.push(s.into_bytes());
        }
    }

    // A integração real dos Providers virá na etapa seguinte.
    let providers = Providers::new()
        .with_host("io", Box::new(providers::io::WebIoProvider))
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
