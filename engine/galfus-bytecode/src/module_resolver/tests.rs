use super::{ModuleResolveContext, ModuleResolveError};
use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath};

fn context() -> ModuleResolveContext {
    ModuleResolveContext::with_path(
        ModuleId::new(91),
        ModulePath::new("src/example.gfs").expect("valid module path"),
    )
}

#[test]
fn every_resolver_error_retains_module_identity_and_has_stable_text() {
    let expected = ContentHash::of(b"expected");
    let actual = ContentHash::of(b"actual");
    let errors = vec![
        ModuleResolveError::UnknownModule {
            context: ModuleResolveContext::new(ModuleId::new(91)),
        },
        ModuleResolveError::UnavailableInEager { context: context() },
        ModuleResolveError::ChunkDecode { context: context() },
        ModuleResolveError::ChunkHashMismatch {
            context: context(),
            expected,
            actual,
        },
        ModuleResolveError::InterfaceMismatch {
            context: context(),
            expected,
            actual,
        },
        ModuleResolveError::DependencyCycle { context: context() },
        ModuleResolveError::ProducerFailed { context: context() },
    ];

    let expected_messages = [
        "module ModuleId(91) is not declared in the module catalog".to_string(),
        "module ModuleId(91) (src/example.gfs) is unavailable for eager materialization"
            .to_string(),
        "module ModuleId(91) (src/example.gfs) has an invalid module chunk".to_string(),
        format!(
            "module ModuleId(91) (src/example.gfs) has a content hash mismatch: expected {expected}, actual {actual}"
        ),
        format!(
            "module ModuleId(91) (src/example.gfs) has an interface hash mismatch: expected {expected}, actual {actual}"
        ),
        "dependency cycle detected while resolving module ModuleId(91) (src/example.gfs)"
            .to_string(),
        "module ModuleId(91) (src/example.gfs) could not be produced".to_string(),
    ];

    for (error, expected_message) in errors.iter().zip(expected_messages) {
        assert_eq!(error.module_id(), ModuleId::new(91));
        assert_eq!(error.to_string(), expected_message);
        assert!(!error.to_string().contains("0x"));
    }

    assert_eq!(errors[0].module_path(), None);
    assert_eq!(
        errors[1].module_path().map(ModulePath::as_str),
        Some("src/example.gfs")
    );
}
