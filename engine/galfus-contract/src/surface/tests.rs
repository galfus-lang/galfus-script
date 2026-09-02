use super::*;

fn user_schema() -> SurfaceSchema {
    SurfaceSchema::Struct {
        name: "User".to_string(),
        fields: vec![SurfaceField {
            name: "name".to_string(),
            schema: SurfaceSchema::Bytes,
        }],
    }
}

#[test]
fn contract_fingerprint_is_stable_for_identical_schema() {
    let first = SurfaceContract::new(
        "std/users::__provider_get_user:return",
        1,
        SurfaceDirection::FromProvider,
        user_schema(),
    );
    let second = SurfaceContract::new(
        "std/users::__provider_get_user:return",
        1,
        SurfaceDirection::FromProvider,
        user_schema(),
    );

    assert_eq!(first.fingerprint, second.fingerprint);
    assert!(first.validates());
}

#[test]
fn contract_fingerprint_changes_with_return_schema() {
    let user = SurfaceContract::new(
        "std/users::__provider_get_user:return",
        1,
        SurfaceDirection::FromProvider,
        user_schema(),
    );
    let changed = SurfaceContract::new(
        "std/users::__provider_get_user:return",
        1,
        SurfaceDirection::FromProvider,
        SurfaceSchema::Struct {
            name: "User".to_string(),
            fields: vec![SurfaceField {
                name: "age".to_string(),
                schema: SurfaceSchema::I32,
            }],
        },
    );

    assert_ne!(user.fingerprint, changed.fingerprint);
}

#[test]
fn registry_binds_host_operation_to_provider_declaration() {
    let result = SurfaceContract::new(
        "std/users::__provider_get_user:return",
        1,
        SurfaceDirection::FromProvider,
        user_schema(),
    );
    let mut registry = SurfaceContractRegistry::default();
    registry
        .register(
            "std/users",
            SurfaceFunctionContract {
                provider_operation: "get_user".to_string(),
                bridge_symbol: "__provider_get_user".to_string(),
                parameters: vec![],
                result,
            },
        )
        .unwrap();

    assert_eq!(
        registry.get("std/users", "get_user").unwrap().bridge_symbol,
        "__provider_get_user"
    );
}

#[test]
fn struct_value_is_validated_by_named_contract_fields() {
    let value = SurfaceValue::Struct(vec![(
        "name".to_string(),
        SurfaceValue::Bytes(b"Name".to_vec()),
    )]);

    assert_eq!(user_schema().validate_value(&value), Ok(()));
}

#[test]
fn struct_value_rejects_missing_contract_field() {
    let value = SurfaceValue::Struct(vec![]);

    assert_eq!(
        user_schema().validate_value(&value),
        Err(SurfaceCodecError::TypeMismatch {
            expected: "struct field count".to_string(),
            found: "0".to_string(),
        })
    );
}

#[test]
fn time_provider_descriptor_exposes_its_surface_contract() {
    let descriptor = crate::std_time_provider_descriptor();
    let module = descriptor.modules.first().unwrap();
    let contract = module.surface_contract("time_now").unwrap();

    assert_eq!(contract.bridge_symbol, "__provider_time_now");
    assert_eq!(contract.result.schema, SurfaceSchema::I64);
    assert!(contract.validates());
}

#[test]
fn provider_result_uses_contract_to_encode_legacy_transport() {
    let descriptor = crate::std_time_provider_descriptor();
    let contract = descriptor.modules[0].surface_contract("time_now").unwrap();

    assert_eq!(
        contract.result.encode_legacy_result(SurfaceValue::I64(42)),
        Ok(crate::BoundaryValue::I64(42))
    );
}
