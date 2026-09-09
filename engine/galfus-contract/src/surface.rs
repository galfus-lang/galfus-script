#[cfg(test)]
mod tests;

use galfus_core::{HandleId, OpaqueTypeId};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Direction of a surface contract relative to a provider function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SurfaceDirection {
    ToProvider,
    FromProvider,
}

/// A closed schema used to exchange structured data with a provider.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum SurfaceSchema {
    Null,
    Bool,
    U16,
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    Bytes,
    Optional(Box<Self>),
    List(Box<Self>),
    Tuple(Vec<Self>),
    Struct {
        name: String,
        fields: Vec<SurfaceField>,
    },
    Choice {
        name: String,
        variants: Vec<SurfaceVariant>,
    },
    Handle {
        resource: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct SurfaceField {
    pub name: String,
    pub schema: SurfaceSchema,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct SurfaceVariant {
    pub name: String,
    pub payload: Option<SurfaceSchema>,
}

/// Contract metadata for one argument or return surface of `__provider_*`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SurfaceContract {
    pub name: String,
    pub version: u32,
    pub direction: SurfaceDirection,
    pub schema: SurfaceSchema,
    pub fingerprint: [u8; 32],
}

impl SurfaceContract {
    pub fn new(
        name: impl Into<String>,
        version: u32,
        direction: SurfaceDirection,
        schema: SurfaceSchema,
    ) -> Self {
        let name = name.into();
        let fingerprint = schema.fingerprint(&name, version, direction);
        Self {
            name,
            version,
            direction,
            schema,
            fingerprint,
        }
    }

    pub fn validates(&self) -> bool {
        self.fingerprint
            == self
                .schema
                .fingerprint(&self.name, self.version, self.direction)
    }
}

/// Surface contracts bound to one `__provider_*` declaration and its host
/// operation name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SurfaceFunctionContract {
    /// Host operation name passed to `HostProvider::dispatch`, such as `time_now`.
    pub provider_operation: String,
    /// Internal Galfus declaration, such as `__provider_time_now`.
    pub bridge_symbol: String,
    pub parameters: Vec<SurfaceContract>,
    pub result: SurfaceContract,
}

impl SurfaceFunctionContract {
    pub fn validates(&self) -> bool {
        self.bridge_symbol.starts_with("__provider_")
            && self.parameters.iter().all(SurfaceContract::validates)
            && self.result.direction == SurfaceDirection::FromProvider
            && self.result.validates()
    }
}

/// Lookup table for contract resolution at provider dispatch and completion.
#[derive(Debug, Clone, Default)]
pub struct SurfaceContractRegistry {
    functions: BTreeMap<(String, String), SurfaceFunctionContract>,
}

impl SurfaceContractRegistry {
    pub fn register(
        &mut self,
        module_path: impl Into<String>,
        contract: SurfaceFunctionContract,
    ) -> Result<(), SurfaceCodecError> {
        if !contract.validates() {
            return Err(SurfaceCodecError::InvalidContract);
        }
        let key = (module_path.into(), contract.provider_operation.clone());
        if self.functions.insert(key, contract).is_some() {
            return Err(SurfaceCodecError::DuplicateContract);
        }
        Ok(())
    }

    pub fn get(
        &self,
        module_path: &str,
        provider_operation: &str,
    ) -> Option<&SurfaceFunctionContract> {
        self.functions
            .get(&(module_path.to_string(), provider_operation.to_string()))
    }
}

impl SurfaceSchema {
    pub fn validate_value(&self, value: &SurfaceValue) -> Result<(), SurfaceCodecError> {
        match (self, value) {
            (Self::Null, SurfaceValue::Null)
            | (Self::Bool, SurfaceValue::Bool(_))
            | (Self::U16, SurfaceValue::U16(_))
            | (Self::I32, SurfaceValue::I32(_))
            | (Self::I64, SurfaceValue::I64(_))
            | (Self::U32, SurfaceValue::U32(_))
            | (Self::U64, SurfaceValue::U64(_))
            | (Self::F32, SurfaceValue::F32(_))
            | (Self::F64, SurfaceValue::F64(_))
            | (Self::Bytes, SurfaceValue::Bytes(_)) => Ok(()),
            (Self::Optional(_), SurfaceValue::Null) => Ok(()),
            (Self::Optional(inner), value) => inner.validate_value(value),
            (Self::List(inner), SurfaceValue::List(values)) => {
                for value in values {
                    inner.validate_value(value)?;
                }
                Ok(())
            }
            (Self::Tuple(schemas), SurfaceValue::Tuple(values))
                if schemas.len() == values.len() =>
            {
                for (schema, value) in schemas.iter().zip(values) {
                    schema.validate_value(value)?;
                }
                Ok(())
            }
            (Self::Struct { fields, .. }, SurfaceValue::Struct(values)) => {
                if fields.len() != values.len() {
                    return Err(SurfaceCodecError::TypeMismatch {
                        expected: "struct field count".to_string(),
                        found: values.len().to_string(),
                    });
                }
                for field in fields {
                    let value = values
                        .iter()
                        .find_map(|(name, value)| (name == &field.name).then_some(value))
                        .ok_or_else(|| SurfaceCodecError::MissingField(field.name.clone()))?;
                    field.schema.validate_value(value)?;
                }
                Ok(())
            }
            (Self::Choice { variants, .. }, SurfaceValue::Choice { variant, payload }) => {
                let expected = variants
                    .iter()
                    .find(|candidate| candidate.name == *variant)
                    .ok_or_else(|| SurfaceCodecError::InvalidTag(variant.clone()))?;
                match (&expected.payload, payload) {
                    (None, None) => Ok(()),
                    (Some(schema), Some(value)) => schema.validate_value(value),
                    _ => Err(SurfaceCodecError::TypeMismatch {
                        expected: format!("choice payload for {variant}"),
                        found: "incompatible payload".to_string(),
                    }),
                }
            }
            (Self::Handle { .. }, SurfaceValue::Handle(_)) => Ok(()),
            (schema, value) => Err(SurfaceCodecError::TypeMismatch {
                expected: format!("{schema:?}"),
                found: format!("{value:?}"),
            }),
        }
    }

    pub fn fingerprint(
        &self,
        contract_name: &str,
        version: u32,
        direction: SurfaceDirection,
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(contract_name.as_bytes());
        hasher.update(version.to_le_bytes());
        hasher.update([direction as u8]);
        self.write_fingerprint(&mut hasher);
        hasher.finalize().into()
    }

    fn write_fingerprint(&self, hasher: &mut Sha256) {
        match self {
            Self::Null => hasher.update([0]),
            Self::Bool => hasher.update([1]),
            Self::U16 => hasher.update([15]),
            Self::I32 => hasher.update([2]),
            Self::I64 => hasher.update([3]),
            Self::U32 => hasher.update([4]),
            Self::U64 => hasher.update([5]),
            Self::F32 => hasher.update([6]),
            Self::F64 => hasher.update([7]),
            Self::Bytes => hasher.update([8]),
            Self::Optional(inner) => {
                hasher.update([9]);
                inner.write_fingerprint(hasher);
            }
            Self::List(inner) => {
                hasher.update([10]);
                inner.write_fingerprint(hasher);
            }
            Self::Tuple(items) => {
                hasher.update([14]);
                hasher.update((items.len() as u64).to_le_bytes());
                for item in items {
                    item.write_fingerprint(hasher);
                }
            }
            Self::Struct { name, fields } => {
                hasher.update([11]);
                hasher.update(name.as_bytes());
                hasher.update((fields.len() as u64).to_le_bytes());
                for field in fields {
                    hasher.update(field.name.as_bytes());
                    field.schema.write_fingerprint(hasher);
                }
            }
            Self::Choice { name, variants } => {
                hasher.update([12]);
                hasher.update(name.as_bytes());
                hasher.update((variants.len() as u64).to_le_bytes());
                for variant in variants {
                    hasher.update(variant.name.as_bytes());
                    match &variant.payload {
                        Some(payload) => {
                            hasher.update([1]);
                            payload.write_fingerprint(hasher);
                        }
                        None => hasher.update([0]),
                    }
                }
            }
            Self::Handle { resource } => {
                hasher.update([13]);
                hasher.update(resource.as_bytes());
            }
        }
    }
}

/// Private, heap-independent data exchanged by the surface codec.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SurfaceValue {
    Null,
    Bool(bool),
    U16(u16),
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bytes(Vec<u8>),
    List(Vec<Self>),
    Tuple(Vec<Self>),
    Struct(Vec<(String, Self)>),
    Choice {
        variant: String,
        payload: Option<Box<Self>>,
    },
    Handle(SurfaceHandle),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SurfaceHandle {
    pub type_id: OpaqueTypeId,
    pub id: HandleId,
}

pub trait EncodeSurface {
    fn encode_surface(&self) -> Result<SurfaceValue, SurfaceCodecError>;
}

pub trait DecodeSurface: Sized {
    fn decode_surface(value: SurfaceValue) -> Result<Self, SurfaceCodecError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SurfaceCodecError {
    #[error("surface contract is invalid")]
    InvalidContract,
    #[error("surface contract is already registered")]
    DuplicateContract,
    #[error("unknown surface contract: {0}")]
    UnknownContract(String),
    #[error("surface contract version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u32, found: u32 },
    #[error("surface schema fingerprint mismatch")]
    FingerprintMismatch,
    #[error("surface field is missing: {0}")]
    MissingField(String),
    #[error("surface choice tag is invalid: {0}")]
    InvalidTag(String),
    #[error("surface type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("surface handle is invalid")]
    InvalidHandle,
}
