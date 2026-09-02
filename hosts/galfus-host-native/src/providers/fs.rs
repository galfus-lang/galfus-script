use galfus_contract::builtins::std_fs_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::path::Path;
use std::sync::Arc;

pub struct NativeFsProvider;
impl Default for NativeFsProvider {
    fn default() -> Self {
        Self::new()
    }
}
impl NativeFsProvider {
    pub fn new() -> Self {
        Self
    }
    fn path(args: &[SurfaceValue]) -> Result<String, ExecutionFailure> {
        match args.first() {
            Some(SurfaceValue::Bytes(bytes)) => std::str::from_utf8(bytes)
                .map(|path| path.replace('\\', "/"))
                .map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid UTF-8 path",
                    )
                }),
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                "expected surface path bytes",
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
    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        let result = (|| -> Result<SurfaceValue, ExecutionFailure> {
            if name == "fs_normalize_path" {
                return Self::path(args).map(|path| SurfaceValue::Bytes(path.into_bytes()));
            }
            let path = Self::path(args)?;
            match name {
                "fs_read" => Ok(std::fs::read(&path)
                    .map(SurfaceValue::Bytes)
                    .unwrap_or(SurfaceValue::Null)),
                "fs_write" => match args {
                    [SurfaceValue::Bytes(_), SurfaceValue::Bytes(data)] => {
                        Ok(SurfaceValue::Bool(std::fs::write(&path, data).is_ok()))
                    }
                    _ => Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "expected surface file path and data",
                    )),
                },
                "fs_exists" => Ok(SurfaceValue::Bool(Path::new(&path).exists())),
                "fs_delete" => {
                    let path = Path::new(&path);
                    Ok(SurfaceValue::Bool(if path.is_dir() {
                        std::fs::remove_dir_all(path).is_ok()
                    } else {
                        std::fs::remove_file(path).is_ok()
                    }))
                }
                "fs_is_directory" => Ok(SurfaceValue::Bool(Path::new(&path).is_dir())),
                "fs_is_file" => Ok(SurfaceValue::Bool(Path::new(&path).is_file())),
                "fs_size" => Ok(SurfaceValue::I64(
                    std::fs::metadata(&path)
                        .map(|metadata| metadata.len() as i64)
                        .unwrap_or(-1),
                )),
                "fs_list" => match std::fs::read_dir(&path) {
                    Ok(entries) => {
                        let mut files = entries
                            .flatten()
                            .filter_map(|entry| entry.file_name().into_string().ok())
                            .map(|name| SurfaceValue::Bytes(name.into_bytes()))
                            .collect::<Vec<_>>();
                        files.sort_by(|left, right| match (left, right) {
                            (SurfaceValue::Bytes(left), SurfaceValue::Bytes(right)) => {
                                left.cmp(right)
                            }
                            _ => std::cmp::Ordering::Equal,
                        });
                        Ok(SurfaceValue::List(files))
                    }
                    Err(_) => Ok(SurfaceValue::Null),
                },
                "fs_mkdir" => Ok(SurfaceValue::Bool(std::fs::create_dir_all(&path).is_ok())),
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "unknown filesystem operation",
                )),
            }
        })();
        let _ = injector.inject_surface_response(thread_id, request_lease, result);
        true
    }
    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
