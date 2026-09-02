use galfus_contract::builtins::std_io_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, SurfaceValue, TaskAffinity,
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
        // Interações com console do navegador e streams requerem a thread principal.
        TaskAffinity::Main
    }

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        match name {
            "io_write" => {
                let bytes = if let Some(BoundaryValue::Bytes(b)) = args.first() {
                    b.clone()
                } else {
                    Vec::new()
                };

                if let Some(stdout) = &self.stdout {
                    let writer = stdout.get_writer().unwrap();
                    let js_bytes = js_sys::Uint8Array::from(bytes.as_slice());

                    wasm_bindgen_futures::spawn_local(async move {
                        let promise = writer.write_with_chunk(&js_bytes);
                        let _ = JsFuture::from(promise).await;
                        writer.release_lock();

                        let _ = injector.inject_system_response(
                            thread_id,
                            request_lease,
                            Ok(BoundaryValue::Null),
                        );
                    });
                } else {
                    // Fallback para console.log
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(text));
                    }
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(BoundaryValue::Null),
                    );
                }
            }
            "io_read" => {
                if let Some(stdin) = &self.stdin {
                    let reader_val = stdin.get_reader();
                    let reader: web_sys::ReadableStreamDefaultReader = reader_val.unchecked_into();

                    wasm_bindgen_futures::spawn_local(async move {
                        let promise = reader.read();
                        let mut result_bytes = Vec::new();

                        if let Ok(js_result) = JsFuture::from(promise).await
                            && let Ok(value) = js_sys::Reflect::get(
                                &js_result,
                                &wasm_bindgen::JsValue::from_str("value"),
                            )
                            && !value.is_undefined()
                            && !value.is_null()
                        {
                            let uint8_arr = js_sys::Uint8Array::new(&value);
                            result_bytes = uint8_arr.to_vec();
                        }

                        reader.release_lock();

                        let _ = injector.inject_system_response(
                            thread_id,
                            request_lease,
                            Ok(BoundaryValue::Bytes(result_bytes)),
                        );
                    });
                } else {
                    // Fallback se não houver stream, retorna vazio.
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(BoundaryValue::Bytes(Vec::new())),
                    );
                }
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Function {} not implemented in WebIoProvider", name),
                    )),
                );
            }
        }
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
                    let js_bytes = js_sys::Uint8Array::from(bytes.as_slice());
                    wasm_bindgen_futures::spawn_local(async move {
                        let promise = writer.write_with_chunk(&js_bytes);
                        let _ = JsFuture::from(promise).await;
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
                    let reader_val = stdin.get_reader();
                    let reader: web_sys::ReadableStreamDefaultReader = reader_val.unchecked_into();
                    wasm_bindgen_futures::spawn_local(async move {
                        let promise = reader.read();
                        let mut result_bytes = Vec::new();
                        if let Ok(js_result) = JsFuture::from(promise).await
                            && let Ok(value) = js_sys::Reflect::get(
                                &js_result,
                                &wasm_bindgen::JsValue::from_str("value"),
                            )
                            && !value.is_undefined()
                            && !value.is_null()
                        {
                            result_bytes = js_sys::Uint8Array::new(&value).to_vec();
                        }
                        reader.release_lock();
                        let _ = injector.inject_surface_response(
                            thread_id,
                            request_lease,
                            Ok(SurfaceValue::Bytes(result_bytes)),
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
            _ => {
                let _ = injector.inject_surface_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "unknown I/O operation",
                    )),
                );
                true
            }
        }
    }

    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        // Se um read/write estiver pendente, tecnicamente poderíamos cancelar a stream.
        // Mas por simplicidade, manteremos unsupported por enquanto.
        CancellationOutcome::Unsupported
    }
}
