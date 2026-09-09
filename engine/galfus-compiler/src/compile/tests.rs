use super::{
    module::{compile_modules, target_module_index},
    resolve::ModuleIndex,
};
use crate::{CompiledModule, CompilerState};
use galfus_contract::CapabilityCatalog;
use galfus_core::{ModuleId, ModulePath, Revision, SourceFile, SourceId};
use galfus_frontend::modules::{
    FrontendModuleKind, FrontendRoots, FrontendSession, FrontendSource, FrontendUpdate,
};
use std::sync::Arc;

fn path(value: &str) -> ModulePath {
    ModulePath::new(value).expect("test module path is valid")
}

#[test]
fn compiler_rejects_missing_cross_module_target() {
    let error = target_module_index(&ModuleIndex::default(), ModuleId::new(99), "src/main.gfs")
        .expect_err("a missing cross-module target must fail compilation");

    assert!(error.to_string().contains(
        "cross-module call target module ModuleId(99) is unavailable while compiling `src/main.gfs`"
    ));
}

#[test]
fn compiler_session_interns_a_generic_choice_shared_by_two_importers() {
    let result = SourceFile::new(
        SourceId::new(1),
        "src/result.gfs".to_string(),
        "export choice Result<T> { Ok(T), Err }".to_string(),
    );
    let left = SourceFile::new(
        SourceId::new(2),
        "src/left.gfs".to_string(),
        "import result from './result'\nexport fn left(value: result::Result<[u8]>): result::Result<[u8]> { return value }".to_string(),
    );
    let right = SourceFile::new(
        SourceId::new(3),
        "src/right.gfs".to_string(),
        "import result from './result'\nexport fn right(value: result::Result<[u8]>): result::Result<[u8]> { return value }".to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(10),
            path: path("src/result.gfs"),
            source: &result,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(11),
            path: path("src/left.gfs"),
            source: &left,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(12),
            path: path("src/right.gfs"),
            source: &right,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);

    let mut modules = frontend
        .modules()
        .iter()
        .map(|module| {
            CompiledModule::new(
                module.id(),
                module.path().clone(),
                module.semantic_revision(),
                module.source().clone(),
                module.graph().clone(),
                module.type_result().cloned(),
                false,
            )
        })
        .collect::<Vec<_>>();
    let mut state = CompilerState::default();
    let images = compile_modules(&mut modules, &mut state, frontend.string_table())
        .expect("modules with the shared generic choice compile");

    assert_eq!(state.generic_choice_layouts.len(), 1);
    let generic_layout_names = images
        .iter()
        .filter(|node| matches!(node.id, id if id == ModuleId::new(11) || id == ModuleId::new(12)))
        .flat_map(|node| node.module.choice_layouts.iter())
        .map(|layout| layout.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(generic_layout_names.len(), 2);
    assert_eq!(generic_layout_names[0], generic_layout_names[1]);
}

#[test]
fn compiler_lowers_a_namespace_imported_choice_pattern() {
    let result = SourceFile::new(
        SourceId::new(1),
        "src/result.gfs".to_string(),
        "export choice Result<T> { Ok(T), Err }".to_string(),
    );
    let reexport = SourceFile::new(
        SourceId::new(2),
        "src/reexport.gfs".to_string(),
        "export import { Result } from './result'".to_string(),
    );
    let consumer = SourceFile::new(
        SourceId::new(3),
        "src/consumer.gfs".to_string(),
        "import result from './reexport'\nexport fn unwrap(value: result::Result<i32>): i32 { return match value { result::Result::Ok(inner) => inner, result::Result::Err => 0 } }".to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(10),
            path: path("src/result.gfs"),
            source: &result,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(11),
            path: path("src/reexport.gfs"),
            source: &reexport,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(12),
            path: path("src/consumer.gfs"),
            source: &consumer,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);

    let mut modules = frontend
        .modules()
        .iter()
        .map(|module| {
            CompiledModule::new(
                module.id(),
                module.path().clone(),
                module.semantic_revision(),
                module.source().clone(),
                module.graph().clone(),
                module.type_result().cloned(),
                false,
            )
        })
        .collect::<Vec<_>>();
    let images = compile_modules(
        &mut modules,
        &mut CompilerState::default(),
        frontend.string_table(),
    )
    .expect("namespace-imported choice pattern compiles");

    let consumer = images
        .iter()
        .find(|node| node.id == ModuleId::new(12))
        .expect("consumer bytecode module is emitted");
    assert!(
        consumer
            .module
            .functions
            .iter()
            .any(|function| function.name == "unwrap")
    );
}

#[test]
fn compiler_lowers_an_imported_generic_constraint_method_result() {
    let stream = SourceFile::new(
        SourceId::new(1),
        "src/stream.gfs".to_string(),
        r#"
export choice StreamResult<T> { Data(T), End }

export constraint ReadStream<T> {
  fn next(self): StreamResult<T>
}
"#
        .to_string(),
    );
    let consumer = SourceFile::new(
        SourceId::new(2),
        "src/consumer.gfs".to_string(),
        r#"
import { ReadStream, StreamResult } from './stream'

export fn read(stream: ReadStream<i32>): i32 {
  const item = stream::next()
  return match item {
    StreamResult::Data(value) => value,
    StreamResult::End => 0,
  }
}
"#
        .to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(10),
            path: path("src/stream.gfs"),
            source: &stream,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(11),
            path: path("src/consumer.gfs"),
            source: &consumer,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );

    let mut modules = frontend
        .modules()
        .iter()
        .map(|module| {
            CompiledModule::new(
                module.id(),
                module.path().clone(),
                module.semantic_revision(),
                module.source().clone(),
                module.graph().clone(),
                module.type_result().cloned(),
                false,
            )
        })
        .collect::<Vec<_>>();

    let images = compile_modules(
        &mut modules,
        &mut CompilerState::default(),
        frontend.string_table(),
    )
    .expect("an imported generic constraint method result must compile");

    assert!(images.iter().any(|module| module.id == ModuleId::new(11)));
}

#[test]
fn compiler_resolves_a_method_on_a_transitively_reachable_field_type() {
    let provider = SourceFile::new(
        SourceId::new(1),
        "src/provider.gfs".to_string(),
        r#"
export struct RequestBody {}

export fn RequestBody::next(self): i32 => 7

export struct Request {
  body: RequestBody
}
"#
        .to_string(),
    );
    let consumer = SourceFile::new(
        SourceId::new(2),
        "src/consumer.gfs".to_string(),
        r#"
import { Request } from './provider'

export fn read(request: Request): i32 => request.body::next()
"#
        .to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(10),
            path: path("src/provider.gfs"),
            source: &provider,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(11),
            path: path("src/consumer.gfs"),
            source: &consumer,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );

    let mut modules = frontend
        .modules()
        .iter()
        .map(|module| {
            CompiledModule::new(
                module.id(),
                module.path().clone(),
                module.semantic_revision(),
                module.source().clone(),
                module.graph().clone(),
                module.type_result().cloned(),
                false,
            )
        })
        .collect::<Vec<_>>();
    compile_modules(
        &mut modules,
        &mut CompilerState::default(),
        frontend.string_table(),
    )
    .expect("a field type reachable from an imported struct resolves its anchored method");
}

#[test]
fn compiler_uses_the_import_origin_for_a_transitively_reachable_method() {
    let first_provider = SourceFile::new(
        SourceId::new(1),
        "src/first_provider.gfs".to_string(),
        r#"
export struct Body {}
export fn Body::next(self): i32 => 1
export struct Request { body: Body }
"#
        .to_string(),
    );
    let second_provider = SourceFile::new(
        SourceId::new(2),
        "src/second_provider.gfs".to_string(),
        r#"
export struct Body {}
export fn Body::next(self): i32 => 2
"#
        .to_string(),
    );
    let consumer = SourceFile::new(
        SourceId::new(3),
        "src/consumer.gfs".to_string(),
        r#"
import { Request } from './first_provider'

export fn read(request: Request): i32 => request.body::next()
"#
        .to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(10),
            path: path("src/first_provider.gfs"),
            source: &first_provider,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(11),
            path: path("src/second_provider.gfs"),
            source: &second_provider,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(12),
            path: path("src/consumer.gfs"),
            source: &consumer,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );

    let mut modules = frontend
        .modules()
        .iter()
        .map(|module| {
            CompiledModule::new(
                module.id(),
                module.path().clone(),
                module.semantic_revision(),
                module.source().clone(),
                module.graph().clone(),
                module.type_result().cloned(),
                false,
            )
        })
        .collect::<Vec<_>>();
    compile_modules(
        &mut modules,
        &mut CompilerState::default(),
        frontend.string_table(),
    )
    .expect("the anchored method is resolved from Request's import origin");
}

#[test]
fn compiler_requires_a_direct_import_for_a_transitively_reachable_type_name() {
    let provider = SourceFile::new(
        SourceId::new(1),
        "src/provider.gfs".to_string(),
        r#"
export struct RequestBody {}
export struct Request { body: RequestBody }
"#
        .to_string(),
    );
    let consumer = SourceFile::new(
        SourceId::new(2),
        "src/consumer.gfs".to_string(),
        r#"
import { Request } from './provider'

export fn makeBody(): RequestBody => new(RequestBody) {}
"#
        .to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(10),
            path: path("src/provider.gfs"),
            source: &provider,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(11),
            path: path("src/consumer.gfs"),
            source: &consumer,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });

    assert!(
        report.diagnostics.has_errors(),
        "RequestBody must not become a local symbol when only Request is imported"
    );
}

#[test]
fn compiler_resolves_a_method_on_a_field_of_a_returned_struct() {
    let provider = SourceFile::new(
        SourceId::new(1),
        "src/provider.gfs".to_string(),
        r#"
export struct RequestBody {}
export fn RequestBody::next(self): i32 => 7
export struct Request { body: RequestBody }
export fn request(): Request => new(Request) { body: new(RequestBody) {} }
"#
        .to_string(),
    );
    let consumer = SourceFile::new(
        SourceId::new(2),
        "src/consumer.gfs".to_string(),
        r#"
import { request } from './provider'

export fn read(): i32 => request().body::next()
"#
        .to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(10),
            path: path("src/provider.gfs"),
            source: &provider,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(11),
            path: path("src/consumer.gfs"),
            source: &consumer,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );

    let mut modules = frontend
        .modules()
        .iter()
        .map(|module| {
            CompiledModule::new(
                module.id(),
                module.path().clone(),
                module.semantic_revision(),
                module.source().clone(),
                module.graph().clone(),
                module.type_result().cloned(),
                false,
            )
        })
        .collect::<Vec<_>>();
    compile_modules(
        &mut modules,
        &mut CompilerState::default(),
        frontend.string_table(),
    )
    .expect("a field type reachable from a returned struct resolves its anchored method");
}
