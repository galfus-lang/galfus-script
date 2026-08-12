pub const ASYNC_SOURCE: &str = include_str!("../builtins/internals/async.gfs");
pub const THREAD_SOURCE: &str = include_str!("../builtins/internals/thread.gfs");
pub const MATH_SOURCE: &str = include_str!("../builtins/internals/math.gfs");
pub const CONSTRAINTS_SOURCE: &str = include_str!("../builtins/internals/constraints.gfs");
pub const ITERABLE_SOURCE: &str = include_str!("../builtins/internals/iterable.gfs");
pub const TEXT_SOURCE: &str = include_str!("../builtins/utilities/text.gfs");
pub const FORMAT_SOURCE: &str = include_str!("../builtins/utilities/format.gfs");
pub const FORMAT_ANSI_SOURCE: &str = include_str!("../builtins/utilities/format/ansi.gfs");
pub const STD_IO_SOURCE: &str = include_str!("../builtins/bridges/io.gfs");
pub const STD_TIME_SOURCE: &str = include_str!("../builtins/bridges/time.gfs");
pub const STD_ENV_SOURCE: &str = include_str!("../builtins/bridges/env.gfs");

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
];

/// Bridge templates for optional host capability modules.
pub static BRIDGE_TEMPLATES: &[(&str, &str)] = &[
    ("std/io", STD_IO_SOURCE),
    ("std/time", STD_TIME_SOURCE),
    ("std/env", STD_ENV_SOURCE),
];

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
                    return_type: BoundaryType::Nullable(Box::new(BoundaryType::Array(Box::new(BoundaryType::U8)))),
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
    ("std/io", STD_IO_SOURCE),
    ("std/time", STD_TIME_SOURCE),
    ("std/env", STD_ENV_SOURCE),
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
