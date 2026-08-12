use galfus_contract::builtins::std_fs_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::path::Path;
use std::sync::Arc;

pub struct NativeFsProvider;

impl NativeFsProvider {
    pub fn new() -> Self {
        Self
    }

    fn normalize_path(path_str: &str) -> String {
        path_str.replace('\\', "/")
    }

    fn extract_path(args: &[BoundaryValue]) -> Result<String, ExecutionFailure> {
        match args.get(0) {
            Some(BoundaryValue::Bytes(bytes)) => match std::str::from_utf8(bytes) {
                Ok(s) => Ok(Self::normalize_path(s)),
                Err(_) => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "Invalid UTF-8 path".to_string(),
                )),
            },
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                "Expected byte array for path".to_string(),
            )),
        }
    }
}

impl HostProvider for NativeFsProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_fs_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Any
    }

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        if name == "fs_normalize_path" {
            let path_result = Self::extract_path(args);
            match path_result {
                Ok(path) => {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(BoundaryValue::Bytes(path.into_bytes())),
                    );
                }
                Err(e) => {
                    let _ = injector.inject_system_response(thread_id, request_lease, Err(e));
                }
            }
            return;
        }

        let path = match Self::extract_path(args) {
            Ok(p) => p,
            Err(e) => {
                let _ = injector.inject_system_response(thread_id, request_lease, Err(e));
                return;
            }
        };

        match name {
            "fs_read" => {
                let result = std::fs::read(&path);
                let response = match result {
                    Ok(bytes) => BoundaryValue::Choice {
                        variant: 0,
                        payload: Some(Box::new(BoundaryValue::Bytes(bytes))),
                    },
                    Err(_) => BoundaryValue::Choice {
                        variant: 1,
                        payload: Some(Box::new(BoundaryValue::Null)),
                    },
                };
                let _ = injector.inject_system_response(thread_id, request_lease, Ok(response));
            }
            "fs_write" => {
                let data = match args.get(1) {
                    Some(BoundaryValue::Bytes(bytes)) => bytes,
                    _ => {
                        let _ = injector.inject_system_response(
                            thread_id,
                            request_lease,
                            Err(ExecutionFailure::new(
                                ExecutionFailureKind::ProviderFailure,
                                "Expected byte array for data".to_string(),
                            )),
                        );
                        return;
                    }
                };
                let success = std::fs::write(&path, data).is_ok();
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bool(success)),
                );
            }
            "fs_exists" => {
                let exists = Path::new(&path).exists();
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bool(exists)),
                );
            }
            "fs_delete" => {
                let p = Path::new(&path);
                let success = if p.is_dir() {
                    std::fs::remove_dir_all(p).is_ok()
                } else {
                    std::fs::remove_file(p).is_ok()
                };
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bool(success)),
                );
            }
            "fs_is_directory" => {
                let is_dir = Path::new(&path).is_dir();
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bool(is_dir)),
                );
            }
            "fs_is_file" => {
                let is_file = Path::new(&path).is_file();
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bool(is_file)),
                );
            }
            "fs_size" => {
                let size = std::fs::metadata(&path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(-1);
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::I64(size)),
                );
            }
            "fs_list" => {
                let result = std::fs::read_dir(&path);
                let response = match result {
                    Ok(entries) => {
                        let mut arr = Vec::new();
                        for entry in entries.flatten() {
                            let file_name = entry.file_name();
                            if let Some(s) = file_name.to_str() {
                                arr.push(BoundaryValue::Bytes(s.as_bytes().to_vec()));
                            }
                        }
                        BoundaryValue::Choice {
                            variant: 0,
                            payload: Some(Box::new(BoundaryValue::Array {
                                element_type: galfus_contract::BoundaryType::Array(Box::new(galfus_contract::BoundaryType::U8)),
                                values: arr,
                            })),
                        }
                    }
                    Err(_) => BoundaryValue::Choice {
                        variant: 1,
                        payload: Some(Box::new(BoundaryValue::Null)),
                    },
                };
                let _ = injector.inject_system_response(thread_id, request_lease, Ok(response));
            }
            "fs_mkdir" => {
                let success = std::fs::create_dir_all(&path).is_ok();
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bool(success)),
                );
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Function {} not implemented in NativeFsProvider", name),
                    )),
                );
            }
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
