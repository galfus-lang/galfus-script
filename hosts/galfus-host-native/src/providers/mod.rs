pub mod env;
pub mod fs;
pub mod io;
pub mod time;

use galfus_bytecode::PackageMetadata;
use galfus_contract::Providers;

pub fn default_providers(metadata: PackageMetadata) -> Providers {
    Providers::new()
        .with_host("io", Box::new(io::NativeIoProvider))
        .with_host("env", Box::new(env::NativeEnvProvider::new(metadata)))
        .with_host("time", Box::new(time::NativeTimeProvider::new()))
        .with_host("fs", Box::new(fs::NativeFsProvider::new()))
}
