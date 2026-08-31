pub const ASYNC_SOURCE: &str = include_str!("../builtins/internals/async.gfs");
pub const THREAD_SOURCE: &str = include_str!("../builtins/internals/thread.gfs");
pub const MATH_SOURCE: &str = include_str!("../builtins/internals/math.gfs");
pub const CONSTRAINTS_SOURCE: &str = include_str!("../builtins/internals/constraints.gfs");
pub const ITERABLE_SOURCE: &str = include_str!("../builtins/internals/iterable.gfs");

pub const TEXT_SOURCE: &str = include_str!("../builtins/utilities/text.gfs");
pub const FORMAT_SOURCE: &str = include_str!("../builtins/utilities/format.gfs");
pub const FORMAT_ANSI_SOURCE: &str = include_str!("../builtins/utilities/format/ansi.gfs");
pub const LOG_SOURCE: &str = include_str!("../builtins/utilities/log.gfs");

pub const STD_IO_SOURCE: &str = include_str!("../builtins/bridges/io.gfs");
pub const STD_TIME_SOURCE: &str = include_str!("../builtins/bridges/time.gfs");
pub const STD_ENV_SOURCE: &str = include_str!("../builtins/bridges/env.gfs");
pub const STD_FS_SOURCE: &str = include_str!("../builtins/bridges/fs.gfs");
pub const STD_NET_SOURCE: &str = include_str!("../builtins/bridges/net.gfs");
pub const STD_HTTP_SOURCE: &str = include_str!("../builtins/bridges/http.gfs");
pub const STD_WEBSOCKET_SOURCE: &str = include_str!("../builtins/bridges/websocket.gfs");
pub const STD_SERVER_SOURCE: &str = include_str!("../builtins/bridges/server.gfs");

/// Internal core modules that are built-in to the VM and auto-inferred by the language engine.
pub static INTERNAL_CORE_MODULES: &[(&str, &str)] = &[
    ("std/async", ASYNC_SOURCE),
    ("std/thread", THREAD_SOURCE),
    ("std/math", MATH_SOURCE),
    ("std/constraints", CONSTRAINTS_SOURCE),
    ("std/iterable", ITERABLE_SOURCE),
];

/// Utility modules that are pure Galfus Script algorithmic libraries without native bridge functions.
pub static UTILITY_MODULES: &[(&str, &str)] = &[
    ("text", TEXT_SOURCE),
    ("format", FORMAT_SOURCE),
    ("format/ansi", FORMAT_ANSI_SOURCE),
    ("log", LOG_SOURCE),
];

/// Bridge templates for optional host capability modules.
pub static BRIDGE_TEMPLATES: &[(&str, &str)] = &[
    ("std/io", STD_IO_SOURCE),
    ("std/time", STD_TIME_SOURCE),
    ("std/env", STD_ENV_SOURCE),
    ("std/fs", STD_FS_SOURCE),
    ("std/net", STD_NET_SOURCE),
    ("std/http", STD_HTTP_SOURCE),
    ("std/websocket", STD_WEBSOCKET_SOURCE),
    ("std/server", STD_SERVER_SOURCE),
];

pub fn is_bridge_template(source: &str) -> bool {
    BRIDGE_TEMPLATES.iter().any(|(name, _)| *name == source)
}

pub fn std_io_provider_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/io".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_IO_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "io_read".to_string(),
                    parameter_types: vec![BoundaryType::Array(Box::new(BoundaryType::U8))],
                    return_type: BoundaryType::Array(Box::new(BoundaryType::U8)),
                },
                ProviderFunctionSignature {
                    name: "io_write".to_string(),
                    parameter_types: vec![BoundaryType::Array(Box::new(BoundaryType::U8))],
                    return_type: BoundaryType::Null,
                },
            ],
        }],
    }
}

pub fn std_time_provider_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/time".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_TIME_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![ProviderFunctionSignature {
                name: "time_now".to_string(),
                parameter_types: vec![],
                return_type: BoundaryType::I64,
            }],
        }],
    }
}

pub fn std_env_provider_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/env".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_ENV_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "env_get".to_string(),
                    parameter_types: vec![BoundaryType::Array(Box::new(BoundaryType::U8))],
                    return_type: BoundaryType::Nullable(Box::new(BoundaryType::Array(Box::new(
                        BoundaryType::U8,
                    )))),
                },
                ProviderFunctionSignature {
                    name: "env_has".to_string(),
                    parameter_types: vec![BoundaryType::Array(Box::new(BoundaryType::U8))],
                    return_type: BoundaryType::Bool,
                },
            ],
        }],
    }
}

pub fn std_fs_provider_descriptor() -> ProviderDescriptor {
    let byte_array = BoundaryType::Array(Box::new(BoundaryType::U8));
    let byte_array_array = BoundaryType::Array(Box::new(byte_array.clone()));

    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/fs".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_FS_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "fs_read".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::Nullable(Box::new(byte_array.clone())),
                },
                ProviderFunctionSignature {
                    name: "fs_write".to_string(),
                    parameter_types: vec![byte_array.clone(), byte_array.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_exists".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_delete".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_is_directory".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_is_file".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_size".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::I64,
                },
                ProviderFunctionSignature {
                    name: "fs_list".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::Nullable(Box::new(byte_array_array)),
                },
                ProviderFunctionSignature {
                    name: "fs_mkdir".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_normalize_path".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: byte_array.clone(),
                },
            ],
        }],
    }
}

pub fn std_net_provider_descriptor() -> ProviderDescriptor {
    let bytes = BoundaryType::Array(Box::new(BoundaryType::U8));
    let datagram = BoundaryType::Tuple(vec![bytes.clone(), bytes.clone(), BoundaryType::U16]);
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/net".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_NET_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "net_tcp_connect".to_string(),
                    parameter_types: vec![bytes.clone(), BoundaryType::U16],
                    return_type: BoundaryType::Nullable(Box::new(BoundaryType::U64)),
                },
                ProviderFunctionSignature {
                    name: "net_tcp_read".to_string(),
                    parameter_types: vec![BoundaryType::U64, BoundaryType::U32],
                    return_type: BoundaryType::Nullable(Box::new(bytes.clone())),
                },
                ProviderFunctionSignature {
                    name: "net_tcp_write".to_string(),
                    parameter_types: vec![BoundaryType::U64, bytes.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "net_tcp_close".to_string(),
                    parameter_types: vec![BoundaryType::U64],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "net_udp_bind".to_string(),
                    parameter_types: vec![bytes.clone(), BoundaryType::U16],
                    return_type: BoundaryType::Nullable(Box::new(BoundaryType::U64)),
                },
                ProviderFunctionSignature {
                    name: "net_udp_receive".to_string(),
                    parameter_types: vec![BoundaryType::U64, BoundaryType::U32],
                    return_type: BoundaryType::Nullable(Box::new(datagram)),
                },
                ProviderFunctionSignature {
                    name: "net_udp_send_to".to_string(),
                    parameter_types: vec![
                        BoundaryType::U64,
                        bytes.clone(),
                        BoundaryType::U16,
                        bytes,
                    ],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "net_udp_close".to_string(),
                    parameter_types: vec![BoundaryType::U64],
                    return_type: BoundaryType::Bool,
                },
            ],
        }],
    }
}

pub fn std_http_provider_descriptor() -> ProviderDescriptor {
    let bytes = BoundaryType::Array(Box::new(BoundaryType::U8));
    let header = BoundaryType::Tuple(vec![bytes.clone(), bytes.clone()]);
    let response = BoundaryType::Tuple(vec![
        BoundaryType::I32,
        BoundaryType::Array(Box::new(header)),
        bytes.clone(),
    ]);
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/http".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_HTTP_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![ProviderFunctionSignature {
                name: "http_request".to_string(),
                parameter_types: vec![
                    bytes.clone(),
                    bytes.clone(),
                    BoundaryType::Array(Box::new(BoundaryType::Tuple(vec![
                        bytes.clone(),
                        bytes.clone(),
                    ]))),
                    BoundaryType::Nullable(Box::new(bytes)),
                ],
                return_type: BoundaryType::Nullable(Box::new(response)),
            }],
        }],
    }
}

pub fn std_websocket_provider_descriptor() -> ProviderDescriptor {
    let bytes = BoundaryType::Array(Box::new(BoundaryType::U8));
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/websocket".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_WEBSOCKET_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "websocket_connect".to_string(),
                    parameter_types: vec![bytes.clone()],
                    return_type: BoundaryType::Nullable(Box::new(BoundaryType::U64)),
                },
                ProviderFunctionSignature {
                    name: "websocket_receive".to_string(),
                    parameter_types: vec![BoundaryType::U64],
                    return_type: BoundaryType::Nullable(Box::new(bytes.clone())),
                },
                ProviderFunctionSignature {
                    name: "websocket_send".to_string(),
                    parameter_types: vec![BoundaryType::U64, bytes],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "websocket_close".to_string(),
                    parameter_types: vec![BoundaryType::U64],
                    return_type: BoundaryType::Bool,
                },
            ],
        }],
    }
}

/// Combined builtin modules for standard workspace lookup.
pub static BUILTIN_MODULES: &[(&str, &str)] = &[
    ("std/async", ASYNC_SOURCE),
    ("std/thread", THREAD_SOURCE),
    ("std/math", MATH_SOURCE),
    ("std/constraints", CONSTRAINTS_SOURCE),
    ("std/iterable", ITERABLE_SOURCE),
    ("text", TEXT_SOURCE),
    ("format", FORMAT_SOURCE),
    ("format/ansi", FORMAT_ANSI_SOURCE),
    ("log", LOG_SOURCE),
];

pub fn is_internal_module(source: &str) -> bool {
    INTERNAL_CORE_MODULES
        .iter()
        .any(|(name, _)| *name == source)
}

pub fn is_utility_module(source: &str) -> bool {
    UTILITY_MODULES.iter().any(|(name, _)| *name == source)
}

pub fn is_builtin_module(source: &str) -> bool {
    BUILTIN_MODULES.iter().any(|(name, _)| *name == source)
}

/// An atomic pairing of a HostProvider and its Galfus `.gfs` interface module.
pub struct BridgeModule {
    pub name: String,
    pub source: String,
}

impl BridgeModule {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
        }
    }
}
use crate::{
    BoundaryType, CURRENT_BOUNDARY_ABI_VERSION, ProviderDescriptor, ProviderFunctionSignature,
    ProviderModuleDescriptor, provider_schema_fingerprint,
};

pub fn std_server_provider_descriptor() -> ProviderDescriptor {
    let bytes = BoundaryType::Array(Box::new(BoundaryType::U8));
    let header = BoundaryType::Tuple(vec![bytes.clone(), bytes.clone()]);

    let url = BoundaryType::Tuple(vec![
        bytes.clone(), // href
        bytes.clone(), // protocol
        bytes.clone(), // host
        bytes.clone(), // hostname
        bytes.clone(), // path
        bytes.clone(), // search
        bytes.clone(), // hash
        bytes.clone(), // origin
    ]);

    let request = BoundaryType::Tuple(vec![
        BoundaryType::U64, // id
        url,
        bytes.clone(),                                   // method
        BoundaryType::Array(Box::new(header.clone())),   // headers
        BoundaryType::Nullable(Box::new(bytes.clone())), // body
    ]);

    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/server".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_SERVER_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "server_bind".to_string(),
                    parameter_types: vec![BoundaryType::I32],
                    return_type: BoundaryType::U64,
                },
                ProviderFunctionSignature {
                    name: "server_accept".to_string(),
                    parameter_types: vec![BoundaryType::U64],
                    return_type: request,
                },
                ProviderFunctionSignature {
                    name: "server_respond".to_string(),
                    parameter_types: vec![
                        BoundaryType::U64,
                        BoundaryType::I32,
                        BoundaryType::Array(Box::new(header)),
                        BoundaryType::Nullable(Box::new(bytes.clone())),
                        BoundaryType::Bool,
                    ],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "server_ws_receive".to_string(),
                    parameter_types: vec![BoundaryType::U64],
                    return_type: BoundaryType::Nullable(Box::new(BoundaryType::Tuple(vec![
                        BoundaryType::I32,
                        BoundaryType::Nullable(Box::new(bytes.clone())),
                    ]))),
                },
                ProviderFunctionSignature {
                    name: "server_ws_send".to_string(),
                    parameter_types: vec![BoundaryType::U64, bytes.clone()],
                    return_type: BoundaryType::Bool,
                },
                ProviderFunctionSignature {
                    name: "server_ws_close".to_string(),
                    parameter_types: vec![BoundaryType::U64],
                    return_type: BoundaryType::Bool,
                },
            ],
        }],
    }
}
