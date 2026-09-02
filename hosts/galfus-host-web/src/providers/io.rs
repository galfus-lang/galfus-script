use galfus_contract::builtins::std_io_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::sync::Arc;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStream, WritableStream};

pub struct WebIoProvider {
    stdin: Option<ReadableStream>,
    stdout: Option<WritableStream>,
}

impl WebIoProvider {
    pub fn new(stdin: Option<ReadableStream>, stdout: Option<WritableStream>) -> Self {
        Self { stdin, stdout }
    }
}

impl HostProvider for WebIoProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_io_provider_descriptor()
    }
    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Main
    }
    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        match name {
            "io_write" => {
                let [SurfaceValue::Bytes(bytes)] = args else {
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "expected surface output bytes",
                        )),
                    );
                    return true;
                };
                if let Some(stdout) = &self.stdout {
                    let writer = stdout.get_writer().unwrap();
                    let bytes = js_sys::Uint8Array::from(bytes.as_slice());
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = JsFuture::from(writer.write_with_chunk(&bytes)).await;
                        writer.release_lock();
                        let _ = injector.inject_surface_response(
                            thread_id,
                            request_lease,
                            Ok(SurfaceValue::Null),
                        );
                    });
                } else {
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(text));
                    }
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Ok(SurfaceValue::Null),
                    );
                }
                true
            }
            "io_read" => {
                if !matches!(args, [SurfaceValue::Bytes(_)]) {
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "expected surface input terminator",
                        )),
                    );
                    return true;
                }
                if let Some(stdin) = &self.stdin {
                    let reader: web_sys::ReadableStreamDefaultReader =
                        stdin.get_reader().unchecked_into();
                    wasm_bindgen_futures::spawn_local(async move {
                        let mut bytes = Vec::new();
                        if let Ok(result) = JsFuture::from(reader.read()).await
                            && let Ok(value) = js_sys::Reflect::get(
                                &result,
                                &wasm_bindgen::JsValue::from_str("value"),
                            )
                            && !value.is_undefined()
                            && !value.is_null()
                        {
                            bytes = js_sys::Uint8Array::new(&value).to_vec();
                        }
                        reader.release_lock();
                        let _ = injector.inject_surface_response(
                            thread_id,
                            request_lease,
                            Ok(SurfaceValue::Bytes(bytes)),
                        );
                    });
                } else {
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Ok(SurfaceValue::Bytes(Vec::new())),
                    );
                }
                true
            }
            _ => false,
        }
    }
    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
