#![no_std]

extern crate alloc;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use galfus_bytecode::PackageImage;
use galfus_contract::{AdapterBindings, ExecutionFailure, KernelDriver, Providers};

pub struct PackageLoader {
    // Embedded-specific loader state (e.g. pointer to flash memory region)
}

impl PackageLoader {
    pub fn new() -> Self {
        Self {}
    }

    pub fn load_from_bytes(&self, bytes: &[u8]) -> Result<PackageImage, String> {
        PackageImage::from_bytecode(bytes).map_err(|e| e.to_string())
    }
}

pub struct ExecutionHost {
    _providers: Providers,
    _adapters: AdapterBindings,
    _driver: Rc<dyn KernelDriver>,
}

impl ExecutionHost {
    pub fn new(
        providers: Providers,
        adapters: AdapterBindings,
        driver: Rc<dyn KernelDriver>,
    ) -> Self {
        Self {
            _providers: providers,
            _adapters: adapters,
            _driver: driver,
        }
    }

    pub fn run(&self, _package: &PackageImage) -> Result<i32, ExecutionFailure> {
        // Here we will bridge to VirtualKernel and Execution
        // The driver may supply an allocator, otherwise we use the default
        todo!("Bridge to VirtualKernel and Execution for embedded");
    }
}
