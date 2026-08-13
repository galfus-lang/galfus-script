use galfus_contract::{AdapterBindings, Providers};
use galfus_host_web::{ExecutionHost, PackageLoader, driver};
use std::rc::Rc;
use std::sync::Arc;

fn main() {
    println!("Initializing Galfus Web Host (WASI/CLI context)...");

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!(
            "No package provided. In a pure Web browser context, the entry point is the exported `start` function called by JS."
        );
        println!(
            "Usage (WASI): {} <package.gfb>",
            args.get(0).unwrap_or(&"galfus-host-web".to_string())
        );
        return;
    }

    let package_path = &args[1];
    let file_bytes = match std::fs::read(package_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to read file {}: {}", package_path, e);
            std::process::exit(1);
        }
    };

    let loader = PackageLoader::new();
    let package = match loader.load_from_bytes(&file_bytes) {
        Ok(pkg) => pkg,
        Err(e) => {
            eprintln!("Failed to load package: {}", e);
            std::process::exit(1);
        }
    };

    let providers = Providers::new(); // Futuro: instanciar providers do web
    let adapters = AdapterBindings::default();

    #[cfg(feature = "multi_thread")]
    let driver: Rc<dyn galfus_runtime::driver::ExecutionDriver> =
        Rc::new(driver::pool::WebWorkersDriver::new());

    #[cfg(not(feature = "multi_thread"))]
    let driver: Rc<dyn galfus_runtime::driver::ExecutionDriver> =
        Rc::new(driver::single::WebKernelDriver::new());

    let host = ExecutionHost::new(providers, adapters, driver);

    let script_args: Vec<Vec<u8>> = std::env::args().skip(1).map(|s| s.into_bytes()).collect();

    match host.run(Arc::new(package), &script_args) {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(e) => {
            eprintln!("Execution failed: {:?}", e);
            std::process::exit(1);
        }
    }
}
