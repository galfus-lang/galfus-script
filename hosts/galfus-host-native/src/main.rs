use galfus_bytecode::PackageImage;
use galfus_contract::AdapterBindings;
use galfus_host_native::ExecutionHost;
use galfus_host_native::driver::NativeDriver;
use std::rc::Rc;
use std::sync::Arc;
const MAGIC_MARKER: &[u8; 8] = b"GLFS_PKG";

fn main() {
    let exe_path = std::env::current_exe().unwrap_or_else(|_| {
        eprintln!("Failed to get current executable path");
        std::process::exit(1);
    });

    let mut package_bytes = None;

    // Tenta procurar pelo payload anexado no fim do binário
    if let Ok(mut file) = std::fs::File::open(&exe_path) {
        use std::io::{Read, Seek, SeekFrom};
        if let Ok(metadata) = file.metadata() {
            let file_size = metadata.len();
            if file_size >= 16 && file.seek(SeekFrom::End(-16)).is_ok() {
                let mut size_buf = [0u8; 8];
                let mut magic_buf = [0u8; 8];

                if file.read_exact(&mut size_buf).is_ok()
                    && file.read_exact(&mut magic_buf).is_ok()
                    && &magic_buf == MAGIC_MARKER
                {
                    let payload_size = u64::from_le_bytes(size_buf);
                    if file_size >= 16 + payload_size
                        && file
                            .seek(SeekFrom::End(-(16 + payload_size as i64)))
                            .is_ok()
                    {
                        let mut buf = vec![0u8; payload_size as usize];
                        if file.read_exact(&mut buf).is_ok() {
                            package_bytes = Some(buf);
                        }
                    }
                }
            }
        }
    }

    let package_bytes = match package_bytes {
        Some(b) => b,
        None => {
            // Fallback para desenvolvimento local: pegar do argumento
            let args: Vec<String> = std::env::args().collect();
            if args.len() < 2 {
                eprintln!("Usage: {} <package.gfb>", args[0]);
                eprintln!(
                    "(Ou execute como um binário empacotado, onde o payload foi anexado no final)"
                );
                std::process::exit(1);
            }
            std::fs::read(&args[1]).unwrap_or_else(|e| {
                eprintln!("Failed to read package {}: {}", args[1], e);
                std::process::exit(1);
            })
        }
    };

    let package = PackageImage::from_bytecode(&package_bytes).unwrap_or_else(|e| {
        eprintln!("Failed to parse package bytecode: {}", e);
        std::process::exit(1);
    });

    let providers = galfus_host_native::providers::default_providers(package.metadata().clone());

    let adapters = AdapterBindings::default();
    let driver = Rc::new(NativeDriver::new());

    let host = ExecutionHost::new(providers, adapters, driver);

    let script_args: Vec<Vec<u8>> = std::env::args().skip(1).map(|s| s.into_bytes()).collect();

    match host.run(Arc::new(package), &script_args) {
        Ok(exit_code) => {
            std::process::exit(exit_code);
        }
        Err(e) => {
            eprintln!("Execution failed: {:?}", e);
            std::process::exit(1);
        }
    }
}
