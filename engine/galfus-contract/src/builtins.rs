use crate::{
    CURRENT_BOUNDARY_ABI_VERSION, ProviderDescriptor, ProviderFunctionSignature,
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
                    parameter_types: vec![SurfaceSchema::Bytes],
                    return_type: SurfaceSchema::Bytes,
                },
                ProviderFunctionSignature {
                    name: "io_write".to_string(),
                    parameter_types: vec![SurfaceSchema::Bytes],
                    return_type: SurfaceSchema::Null,
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
                return_type: SurfaceSchema::I64,
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
                    parameter_types: vec![SurfaceSchema::Bytes],
                    return_type: SurfaceSchema::Optional(Box::new(SurfaceSchema::List(Box::new(
                        SurfaceSchema::U32,
                    )))),
                },
                ProviderFunctionSignature {
                    name: "env_has".to_string(),
                    parameter_types: vec![SurfaceSchema::Bytes],
                    return_type: SurfaceSchema::Bool,
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
    let byte_array = SurfaceSchema::Bytes;
    let byte_array_array = SurfaceSchema::List(Box::new(byte_array.clone()));

    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/fs".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_FS_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "fs_read".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::Optional(Box::new(byte_array.clone())),
                },
                ProviderFunctionSignature {
                    name: "fs_write".to_string(),
                    parameter_types: vec![byte_array.clone(), byte_array.clone()],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_exists".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_delete".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_is_directory".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_is_file".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "fs_size".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::I64,
                },
                ProviderFunctionSignature {
                    name: "fs_list".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::Optional(Box::new(byte_array_array)),
                },
                ProviderFunctionSignature {
                    name: "fs_mkdir".to_string(),
                    parameter_types: vec![byte_array.clone()],
                    return_type: SurfaceSchema::Bool,
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
    let bytes = SurfaceSchema::Bytes;
    let datagram = SurfaceSchema::Tuple(vec![bytes.clone(), bytes.clone(), SurfaceSchema::U16]);
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/net".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_NET_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "net_tcp_connect".to_string(),
                    parameter_types: vec![bytes.clone(), SurfaceSchema::U16],
                    return_type: SurfaceSchema::Optional(Box::new(SurfaceSchema::U64)),
                },
                ProviderFunctionSignature {
                    name: "net_tcp_read".to_string(),
                    parameter_types: vec![SurfaceSchema::U64, SurfaceSchema::U32],
                    return_type: SurfaceSchema::Optional(Box::new(bytes.clone())),
                },
                ProviderFunctionSignature {
                    name: "net_tcp_write".to_string(),
                    parameter_types: vec![SurfaceSchema::U64, bytes.clone()],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "net_tcp_finish".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "net_tcp_close".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "net_udp_bind".to_string(),
                    parameter_types: vec![bytes.clone(), SurfaceSchema::U16],
                    return_type: SurfaceSchema::Optional(Box::new(SurfaceSchema::U64)),
                },
                ProviderFunctionSignature {
                    name: "net_udp_receive".to_string(),
                    parameter_types: vec![SurfaceSchema::U64, SurfaceSchema::U32],
                    return_type: SurfaceSchema::Optional(Box::new(datagram)),
                },
                ProviderFunctionSignature {
                    name: "net_udp_send_to".to_string(),
                    parameter_types: vec![
                        SurfaceSchema::U64,
                        bytes.clone(),
                        SurfaceSchema::U16,
                        bytes,
                    ],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "net_udp_close".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Bool,
                },
            ],
            surface_contracts: net_surface_contracts(),
        }],
    }
}

fn net_surface_contracts() -> Vec<SurfaceFunctionContract> {
    let parameter = |operation: &str, name: &str, schema| {
        SurfaceContract::new(
            format!("std/net::__provider_{operation}:{name}"),
            1,
            SurfaceDirection::ToProvider,
            schema,
        )
    };
    let result = |operation: &str, schema| {
        SurfaceContract::new(
            format!("std/net::__provider_{operation}:return"),
            1,
            SurfaceDirection::FromProvider,
            schema,
        )
    };
    let optional_u64 = SurfaceSchema::Optional(Box::new(SurfaceSchema::U64));
    let optional_bytes = SurfaceSchema::Optional(Box::new(SurfaceSchema::Bytes));
    let datagram = SurfaceSchema::Optional(Box::new(SurfaceSchema::Tuple(vec![
        SurfaceSchema::Bytes,
        SurfaceSchema::Bytes,
        SurfaceSchema::U16,
    ])));
    vec![
        SurfaceFunctionContract {
            provider_operation: "net_tcp_connect".to_string(),
            bridge_symbol: "__provider_net_tcp_connect".to_string(),
            parameters: vec![
                parameter("net_tcp_connect", "host", SurfaceSchema::Bytes),
                parameter("net_tcp_connect", "port", SurfaceSchema::U16),
            ],
            result: result("net_tcp_connect", optional_u64.clone()),
        },
        SurfaceFunctionContract {
            provider_operation: "net_tcp_read".to_string(),
            bridge_symbol: "__provider_net_tcp_read".to_string(),
            parameters: vec![
                parameter("net_tcp_read", "socket", SurfaceSchema::U64),
                parameter("net_tcp_read", "max_bytes", SurfaceSchema::U32),
            ],
            result: result("net_tcp_read", optional_bytes.clone()),
        },
        SurfaceFunctionContract {
            provider_operation: "net_tcp_write".to_string(),
            bridge_symbol: "__provider_net_tcp_write".to_string(),
            parameters: vec![
                parameter("net_tcp_write", "socket", SurfaceSchema::U64),
                parameter("net_tcp_write", "data", SurfaceSchema::Bytes),
            ],
            result: result("net_tcp_write", SurfaceSchema::Bool),
        },
        SurfaceFunctionContract {
            provider_operation: "net_tcp_finish".to_string(),
            bridge_symbol: "__provider_net_tcp_finish".to_string(),
            parameters: vec![parameter("net_tcp_finish", "socket", SurfaceSchema::U64)],
            result: result("net_tcp_finish", SurfaceSchema::Bool),
        },
        SurfaceFunctionContract {
            provider_operation: "net_tcp_close".to_string(),
            bridge_symbol: "__provider_net_tcp_close".to_string(),
            parameters: vec![parameter("net_tcp_close", "socket", SurfaceSchema::U64)],
            result: result("net_tcp_close", SurfaceSchema::Bool),
        },
        SurfaceFunctionContract {
            provider_operation: "net_udp_bind".to_string(),
            bridge_symbol: "__provider_net_udp_bind".to_string(),
            parameters: vec![
                parameter("net_udp_bind", "host", SurfaceSchema::Bytes),
                parameter("net_udp_bind", "port", SurfaceSchema::U16),
            ],
            result: result("net_udp_bind", optional_u64),
        },
        SurfaceFunctionContract {
            provider_operation: "net_udp_receive".to_string(),
            bridge_symbol: "__provider_net_udp_receive".to_string(),
            parameters: vec![
                parameter("net_udp_receive", "socket", SurfaceSchema::U64),
                parameter("net_udp_receive", "max_bytes", SurfaceSchema::U32),
            ],
            result: result("net_udp_receive", datagram),
        },
        SurfaceFunctionContract {
            provider_operation: "net_udp_send_to".to_string(),
            bridge_symbol: "__provider_net_udp_send_to".to_string(),
            parameters: vec![
                parameter("net_udp_send_to", "socket", SurfaceSchema::U64),
                parameter("net_udp_send_to", "host", SurfaceSchema::Bytes),
                parameter("net_udp_send_to", "port", SurfaceSchema::U16),
                parameter("net_udp_send_to", "data", SurfaceSchema::Bytes),
            ],
            result: result("net_udp_send_to", SurfaceSchema::Bool),
        },
        SurfaceFunctionContract {
            provider_operation: "net_udp_close".to_string(),
            bridge_symbol: "__provider_net_udp_close".to_string(),
            parameters: vec![parameter("net_udp_close", "socket", SurfaceSchema::U64)],
            result: result("net_udp_close", SurfaceSchema::Bool),
        },
    ]
}

pub fn std_http_provider_descriptor() -> ProviderDescriptor {
    let bytes = SurfaceSchema::Bytes;
    let header = SurfaceSchema::Handle {
        resource: "std/http.gfs.Header".to_string(),
    };
    let response = SurfaceSchema::Handle {
        resource: "std/http.gfs.ProviderResponse".to_string(),
    };
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/http".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_HTTP_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "http_request".to_string(),
                    parameter_types: vec![
                        bytes.clone(),
                        bytes.clone(),
                        SurfaceSchema::List(Box::new(header.clone())),
                        SurfaceSchema::Optional(Box::new(bytes.clone())),
                    ],
                    return_type: SurfaceSchema::Optional(Box::new(response.clone())),
                },
                ProviderFunctionSignature {
                    name: "http_response_read".to_string(),
                    parameter_types: vec![SurfaceSchema::U64, SurfaceSchema::U32],
                    return_type: SurfaceSchema::Optional(Box::new(bytes.clone())),
                },
                ProviderFunctionSignature {
                    name: "http_response_close".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Bool,
                },
            ],
            surface_contracts: vec![
                SurfaceFunctionContract {
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
                            name: "ProviderResponse".to_string(),
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
                                    schema: SurfaceSchema::U64,
                                },
                            ],
                        })),
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "http_response_read".to_string(),
                    bridge_symbol: "__provider_http_response_read".to_string(),
                    parameters: vec![
                        SurfaceContract::new(
                            "std/http::__provider_http_response_read:body",
                            1,
                            SurfaceDirection::ToProvider,
                            SurfaceSchema::U64,
                        ),
                        SurfaceContract::new(
                            "std/http::__provider_http_response_read:max_bytes",
                            1,
                            SurfaceDirection::ToProvider,
                            SurfaceSchema::U32,
                        ),
                    ],
                    result: SurfaceContract::new(
                        "std/http::__provider_http_response_read:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Optional(Box::new(SurfaceSchema::Bytes)),
                    ),
                },
                SurfaceFunctionContract {
                    provider_operation: "http_response_close".to_string(),
                    bridge_symbol: "__provider_http_response_close".to_string(),
                    parameters: vec![SurfaceContract::new(
                        "std/http::__provider_http_response_close:body",
                        1,
                        SurfaceDirection::ToProvider,
                        SurfaceSchema::U64,
                    )],
                    result: SurfaceContract::new(
                        "std/http::__provider_http_response_close:return",
                        1,
                        SurfaceDirection::FromProvider,
                        SurfaceSchema::Bool,
                    ),
                },
            ],
        }],
    }
}

pub fn std_websocket_provider_descriptor() -> ProviderDescriptor {
    let bytes = SurfaceSchema::Bytes;
    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/websocket".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_WEBSOCKET_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "websocket_connect".to_string(),
                    parameter_types: vec![bytes.clone()],
                    return_type: SurfaceSchema::Optional(Box::new(SurfaceSchema::U64)),
                },
                ProviderFunctionSignature {
                    name: "websocket_receive".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Optional(Box::new(bytes.clone())),
                },
                ProviderFunctionSignature {
                    name: "websocket_send".to_string(),
                    parameter_types: vec![SurfaceSchema::U64, bytes],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "websocket_close".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Bool,
                },
            ],
            surface_contracts: websocket_surface_contracts(),
        }],
    }
}

fn websocket_surface_contracts() -> Vec<SurfaceFunctionContract> {
    vec![
        SurfaceFunctionContract {
            provider_operation: "websocket_connect".to_string(),
            bridge_symbol: "__provider_websocket_connect".to_string(),
            parameters: vec![SurfaceContract::new(
                "std/websocket::__provider_websocket_connect:url",
                1,
                SurfaceDirection::ToProvider,
                SurfaceSchema::Bytes,
            )],
            result: SurfaceContract::new(
                "std/websocket::__provider_websocket_connect:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Optional(Box::new(SurfaceSchema::U64)),
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "websocket_receive".to_string(),
            bridge_symbol: "__provider_websocket_receive".to_string(),
            parameters: vec![SurfaceContract::new(
                "std/websocket::__provider_websocket_receive:socket",
                1,
                SurfaceDirection::ToProvider,
                SurfaceSchema::U64,
            )],
            result: SurfaceContract::new(
                "std/websocket::__provider_websocket_receive:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Optional(Box::new(SurfaceSchema::Bytes)),
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "websocket_send".to_string(),
            bridge_symbol: "__provider_websocket_send".to_string(),
            parameters: vec![
                SurfaceContract::new(
                    "std/websocket::__provider_websocket_send:socket",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::U64,
                ),
                SurfaceContract::new(
                    "std/websocket::__provider_websocket_send:data",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::Bytes,
                ),
            ],
            result: SurfaceContract::new(
                "std/websocket::__provider_websocket_send:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Bool,
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "websocket_close".to_string(),
            bridge_symbol: "__provider_websocket_close".to_string(),
            parameters: vec![SurfaceContract::new(
                "std/websocket::__provider_websocket_close:socket",
                1,
                SurfaceDirection::ToProvider,
                SurfaceSchema::U64,
            )],
            result: SurfaceContract::new(
                "std/websocket::__provider_websocket_close:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Bool,
            ),
        },
    ]
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
    let bytes = SurfaceSchema::Bytes;
    let header = SurfaceSchema::Tuple(vec![bytes.clone(), bytes.clone()]);

    let request = SurfaceSchema::Handle {
        resource: "std/server.gfs.Request".to_string(),
    };
    let ws_message = SurfaceSchema::Handle {
        resource: "std/server.gfs.WsMessage".to_string(),
    };

    ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: "std/server".to_string(),
            schema_fingerprint: provider_schema_fingerprint(STD_SERVER_SOURCE),
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![
                ProviderFunctionSignature {
                    name: "server_bind".to_string(),
                    parameter_types: vec![SurfaceSchema::I32],
                    return_type: SurfaceSchema::U64,
                },
                ProviderFunctionSignature {
                    name: "server_accept".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: request,
                },
                ProviderFunctionSignature {
                    name: "server_respond".to_string(),
                    parameter_types: vec![
                        SurfaceSchema::U64,
                        SurfaceSchema::I32,
                        SurfaceSchema::List(Box::new(header)),
                        SurfaceSchema::Optional(Box::new(bytes.clone())),
                        SurfaceSchema::Bool,
                    ],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "server_ws_receive".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Optional(Box::new(ws_message)),
                },
                ProviderFunctionSignature {
                    name: "server_ws_send".to_string(),
                    parameter_types: vec![SurfaceSchema::U64, bytes.clone()],
                    return_type: SurfaceSchema::Bool,
                },
                ProviderFunctionSignature {
                    name: "server_ws_close".to_string(),
                    parameter_types: vec![SurfaceSchema::U64],
                    return_type: SurfaceSchema::Bool,
                },
            ],
            surface_contracts: server_surface_contracts(),
        }],
    }
}

fn server_surface_contracts() -> Vec<SurfaceFunctionContract> {
    let bytes = SurfaceSchema::Bytes;
    let header = SurfaceSchema::Tuple(vec![bytes.clone(), bytes.clone()]);
    let url = SurfaceSchema::Struct {
        name: "URL".to_string(),
        fields: vec![
            SurfaceField {
                name: "href".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "protocol".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "host".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "hostname".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "pathname".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "search".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "hash".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "origin".to_string(),
                schema: bytes.clone(),
            },
        ],
    };
    let request = SurfaceSchema::Struct {
        name: "Request".to_string(),
        fields: vec![
            SurfaceField {
                name: "id".to_string(),
                schema: SurfaceSchema::U64,
            },
            SurfaceField {
                name: "url".to_string(),
                schema: url,
            },
            SurfaceField {
                name: "method".to_string(),
                schema: bytes.clone(),
            },
            SurfaceField {
                name: "headers".to_string(),
                schema: SurfaceSchema::List(Box::new(header.clone())),
            },
            SurfaceField {
                name: "body".to_string(),
                schema: SurfaceSchema::Optional(Box::new(bytes.clone())),
            },
        ],
    };
    let ws_message = SurfaceSchema::Struct {
        name: "WsMessage".to_string(),
        fields: vec![
            SurfaceField {
                name: "status".to_string(),
                schema: SurfaceSchema::I32,
            },
            SurfaceField {
                name: "msg".to_string(),
                schema: SurfaceSchema::Optional(Box::new(bytes.clone())),
            },
        ],
    };
    vec![
        SurfaceFunctionContract {
            provider_operation: "server_bind".to_string(),
            bridge_symbol: "__provider_server_bind".to_string(),
            parameters: vec![SurfaceContract::new(
                "std/server::__provider_server_bind:port",
                1,
                SurfaceDirection::ToProvider,
                SurfaceSchema::I32,
            )],
            result: SurfaceContract::new(
                "std/server::__provider_server_bind:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::U64,
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "server_accept".to_string(),
            bridge_symbol: "__provider_server_accept".to_string(),
            parameters: vec![SurfaceContract::new(
                "std/server::__provider_server_accept:server_id",
                1,
                SurfaceDirection::ToProvider,
                SurfaceSchema::U64,
            )],
            result: SurfaceContract::new(
                "std/server::__provider_server_accept:return",
                1,
                SurfaceDirection::FromProvider,
                request,
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "server_respond".to_string(),
            bridge_symbol: "__provider_server_respond".to_string(),
            parameters: vec![
                SurfaceContract::new(
                    "std/server::__provider_server_respond:req_id",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::U64,
                ),
                SurfaceContract::new(
                    "std/server::__provider_server_respond:status",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::I32,
                ),
                SurfaceContract::new(
                    "std/server::__provider_server_respond:headers",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::List(Box::new(header)),
                ),
                SurfaceContract::new(
                    "std/server::__provider_server_respond:body",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::Optional(Box::new(bytes.clone())),
                ),
                SurfaceContract::new(
                    "std/server::__provider_server_respond:is_upgrade",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::Bool,
                ),
            ],
            result: SurfaceContract::new(
                "std/server::__provider_server_respond:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Bool,
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "server_ws_receive".to_string(),
            bridge_symbol: "__provider_server_ws_receive".to_string(),
            parameters: vec![SurfaceContract::new(
                "std/server::__provider_server_ws_receive:ws_id",
                1,
                SurfaceDirection::ToProvider,
                SurfaceSchema::U64,
            )],
            result: SurfaceContract::new(
                "std/server::__provider_server_ws_receive:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Optional(Box::new(ws_message)),
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "server_ws_send".to_string(),
            bridge_symbol: "__provider_server_ws_send".to_string(),
            parameters: vec![
                SurfaceContract::new(
                    "std/server::__provider_server_ws_send:ws_id",
                    1,
                    SurfaceDirection::ToProvider,
                    SurfaceSchema::U64,
                ),
                SurfaceContract::new(
                    "std/server::__provider_server_ws_send:data",
                    1,
                    SurfaceDirection::ToProvider,
                    bytes.clone(),
                ),
            ],
            result: SurfaceContract::new(
                "std/server::__provider_server_ws_send:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Bool,
            ),
        },
        SurfaceFunctionContract {
            provider_operation: "server_ws_close".to_string(),
            bridge_symbol: "__provider_server_ws_close".to_string(),
            parameters: vec![SurfaceContract::new(
                "std/server::__provider_server_ws_close:ws_id",
                1,
                SurfaceDirection::ToProvider,
                SurfaceSchema::U64,
            )],
            result: SurfaceContract::new(
                "std/server::__provider_server_ws_close:return",
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Bool,
            ),
        },
    ]
}
