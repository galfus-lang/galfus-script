use crate::{
    BoundaryType, CURRENT_BOUNDARY_ABI_VERSION, ProviderDescriptor, ProviderFunctionSignature,
    ProviderModuleDescriptor, SurfaceContract, SurfaceDirection, SurfaceField,
    SurfaceFunctionContract, SurfaceSchema, provider_schema_fingerprint,
};

pub const ASYNC_SOURCE: &str = include_str!("../builtins/internals/async.gfs");
pub const THREAD_SOURCE: &str = include_str!("../builtins/internals/thread.gfs");
pub const MATH_SOURCE: &str = include_str!("../builtins/internals/math.gfs");
pub const CONSTRAINTS_SOURCE: &str = include_str!("../builtins/internals/constraints.gfs");
pub const ITERABLE_SOURCE: &str = include_str!("../builtins/internals/iterable.gfs");

pub const TEXT_SOURCE: &str = include_str!("../builtins/utilities/text.gfs");
pub const FORMAT_SOURCE: &str = include_str!("../builtins/utilities/format.gfs");
pub const FORMAT_ANSI_SOURCE: &str = include_str!("../builtins/utilities/format/ansi.gfs");
pub const LOG_SOURCE: &str = include_str!("../builtins/utilities/log.gfs");
pub const STREAM_SOURCE: &str = include_str!("../builtins/utilities/stream.gfs");

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
    ("std/stream", STREAM_SOURCE),
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
            surface_contracts: vec![
                SurfaceFunctionContract {
                    provider_operation: "io_read".to_string(),
                    bridge_symbol: "__provider_io_read".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/io::__provider_io_read:terminator",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/io::__provider_io_read:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bytes,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "io_write".to_string(),
                    bridge_symbol: "__provider_io_write".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/io::__provider_io_write:text",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/io::__provider_io_write:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Null,
                    ),
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
            surface_contracts: vec![SurfaceFunctionContract {
                provider_operation: "time_now".to_string(),
                bridge_symbol: "__provider_time_now".to_string(),
                parameters: vec![],
                result: SurfaceContract::new(
                    "std/time::__provider_time_now:return",
                    1,
                    SurfaceDirection::FromProvider,
                    SurfaceSchema::I64,
                ),
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
            surface_contracts: vec![
                SurfaceFunctionContract {
                    provider_operation: "env_get".to_string(),
                    bridge_symbol: "__provider_env_get".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/env::__provider_env_get:key",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/env::__provider_env_get:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Optional(Box::new(SurfaceSchema::Bytes)),
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "env_has".to_string(),
                    bridge_symbol: "__provider_env_has".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/env::__provider_env_has:key",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/env::__provider_env_has:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
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
            surface_contracts: vec![
                SurfaceFunctionContract {
                    provider_operation: "fs_read".to_string(),
                    bridge_symbol: "__provider_fs_read".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_read:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_read:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Optional(Box::new(SurfaceSchema::Bytes)),
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_write".to_string(),
                    bridge_symbol: "__provider_fs_write".to_string(),
                    parameters: vec![
                        SurfaceContract::new(
                            "std/fs::__provider_fs_write:path",
                            1,
                            SurfaceDirection::ToProvider,
                            SurfaceSchema::Bytes,
                        ),
                        SurfaceContract::new(
                            "std/fs::__provider_fs_write:data",
                            1,
                            SurfaceDirection::ToProvider,
                            SurfaceSchema::Bytes,
                        ),
                    ],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_write:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_exists".to_string(),
                    bridge_symbol: "__provider_fs_exists".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_exists:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_exists:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_delete".to_string(),
                    bridge_symbol: "__provider_fs_delete".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_delete:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_delete:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_is_directory".to_string(),
                    bridge_symbol: "__provider_fs_is_directory".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_is_directory:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_is_directory:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_is_file".to_string(),
                    bridge_symbol: "__provider_fs_is_file".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_is_file:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_is_file:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_size".to_string(),
                    bridge_symbol: "__provider_fs_size".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_size:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_size:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::I64,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_list".to_string(),
                    bridge_symbol: "__provider_fs_list".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_list:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_list:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Optional(Box::new(SurfaceSchema::List(Box::new(
                            SurfaceSchema::Bytes,
                        )))),
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_mkdir".to_string(),
                    bridge_symbol: "__provider_fs_mkdir".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_mkdir:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_mkdir:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "fs_normalize_path".to_string(),
                    bridge_symbol: "__provider_fs_normalize_path".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/fs::__provider_fs_normalize_path:path",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    )],
                    result: SurfaceContract::new(
                        "std/fs::__provider_fs_normalize_path:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bytes,
                    ),
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
            surface_contracts: vec![],
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
            surface_contracts: vec![SurfaceFunctionContract {
                provider_operation: "http_request".to_string(),
                bridge_symbol: "__provider_http_request".to_string(),
                parameters: vec![
                    SurfaceContract::new(
                        "std/http::__provider_http_request:method",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    ),
                    SurfaceContract::new(
                        "std/http::__provider_http_request:url",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Bytes,
                    ),
                    SurfaceContract::new(
                        "std/http::__provider_http_request:headers",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::List(Box::new(SurfaceSchema::Struct {
                            name: "Header".to_string(),
                            fields: vec![
                                SurfaceField {
                                    name: "name".to_string(),
                                    schema: SurfaceSchema::Bytes,
                                },
                                SurfaceField {
                                    name: "value".to_string(),
                                    schema: SurfaceSchema::Bytes,
                                },
                            ],
                        })),
                    ),
                    SurfaceContract::new(
                        "std/http::__provider_http_request:body",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::Optional(Box::new(SurfaceSchema::Bytes)),
                    ),
                ],
                result: SurfaceContract::new(
                    "std/http::__provider_http_request:return",
                    1,
                    SurfaceDirection::FromProvider,
                    SurfaceSchema::Optional(Box::new(SurfaceSchema::Struct {
                        name: "Response".to_string(),
                        fields: vec![
                            SurfaceField {
                                name: "status".to_string(),
                                schema: SurfaceSchema::I32,
                            },
                            SurfaceField {
                                name: "headers".to_string(),
                                schema: SurfaceSchema::List(Box::new(SurfaceSchema::Struct {
                                    name: "Header".to_string(),
                                    fields: vec![
                                        SurfaceField {
                                            name: "name".to_string(),
                                            schema: SurfaceSchema::Bytes,
                                        },
                                        SurfaceField {
                                            name: "value".to_string(),
                                            schema: SurfaceSchema::Bytes,
                                        },
                                    ],
                                })),
                            },
                            SurfaceField {
                                name: "body".to_string(),
                                schema: SurfaceSchema::Bytes,
                            },
                        ],
                    })),
                ),
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
            surface_contracts: vec![],
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
    ("std/stream", STREAM_SOURCE),
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
            surface_contracts: vec![],
        }],
    }
}
