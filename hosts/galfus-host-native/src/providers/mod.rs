pub mod env;
pub mod fs;
pub mod http;
pub mod io;
pub mod net;
pub mod server;
pub mod time;
pub mod websocket;

use galfus_bytecode::PackageMetadata;
use galfus_contract::Providers;

pub fn default_providers(metadata: PackageMetadata) -> Providers {
    Providers::new()
        .with_host("io", Box::new(io::NativeIoProvider))
        .with_host("env", Box::new(env::NativeEnvProvider::new(metadata)))
        .with_host("time", Box::new(time::NativeTimeProvider::new()))
        .with_host("fs", Box::new(fs::NativeFsProvider::new()))
        .with_host("net", Box::new(net::NativeNetProvider::new()))
        .with_host("http", Box::new(http::NativeHttpProvider::new()))
        .with_host(
            "websocket",
            Box::new(websocket::NativeWebSocketProvider::new()),
        )
        .with_host("server", Box::new(server::NativeServerProvider::new()))
}
