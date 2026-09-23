use super::*;
use crate::instruction::{FuncIdx, GlobalIdx, Instruction, TypeIdx};
use crate::{
    BytecodeFunction, BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType, ConstantPool,
    ExportKind, ExportSlot, ImportEdge, ImportKind, ImportSlot,
};
use galfus_core::{ModuleId, ModulePath, RuntimeExportId, RuntimeExportKind, SemanticRevision};

fn node(id: ModuleId, path: &str, module: BytecodeModule) -> BytecodeNode {
    BytecodeNode {
        id,
        path: ModulePath::new(path).expect("valid module path"),
        semantic_revision: SemanticRevision::new(0),
        module,
        metadata: None,
    }
}

fn module(imports: Vec<ImportSlot>, exports: Vec<ExportSlot>, global_count: u32) -> BytecodeModule {
    BytecodeModule {
        name: "test.gfs".to_string(),
        global_count,
        constants: ConstantPool::default(),
        functions: vec![BytecodeFunction {
            name: "value".to_string(),
            param_count: 0,
            local_count: 0,
            temp_count: 0,
            return_ty: TypeIdx(0),
            adapter_proxy_metadata: None,
            instructions: vec![Instruction::RetNull],
        }],
        types: vec![BytecodeType::Null],
        struct_layouts: Vec::new(),
        choice_layouts: Vec::new(),
        imports,
        exports,
        init_func_idx: None,
    }
}

fn import(
    target_module_id: Option<ModuleId>,
    target_export_id: Option<RuntimeExportId>,
    kind: ImportKind,
) -> ImportSlot {
    ImportSlot {
        module_name: "target.gfs".to_string(),
        symbol_name: "value".to_string(),
        ty: TypeIdx(0),
        kind,
        target_module_id,
        target_export_id,
    }
}

#[test]
fn direct_mode_resolves_function_imports_by_module_and_export_ids() {
    let importer = ModuleId::new(7);
    let target = ModuleId::new(31);
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![
            node(
                importer,
                "importer.gfs",
                module(
                    vec![import(
                        Some(target),
                        Some(RuntimeExportId::new(
                            target,
                            RuntimeExportKind::Function,
                            "value",
                        )),
                        ImportKind::Function,
                    )],
                    Vec::new(),
                    0,
                ),
            ),
            node(
                target,
                "target.gfs",
                module(
                    Vec::new(),
                    vec![ExportSlot {
                        symbol_name: "value".to_string(),
                        kind: ExportKind::Function(FuncIdx(0)),
                    }],
                    0,
                ),
            ),
        ],
        vec![ImportEdge {
            from: importer,
            to: target,
        }],
    )
    .expect("valid direct function graph");

    assert_eq!(
        graph
            .resolve_imports(importer)
            .expect("direct import resolves")
            .imports,
        vec![ResolvedImport {
            slot: 0,
            module_id: target,
            kind: ResolvedImportKind::Function(FuncIdx(0)),
        }]
    );
}

#[test]
fn direct_mode_resolves_global_imports_by_module_and_export_ids() {
    let importer = ModuleId::new(7);
    let target = ModuleId::new(31);
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![
            node(
                importer,
                "importer.gfs",
                module(
                    vec![import(
                        Some(target),
                        Some(RuntimeExportId::new(
                            target,
                            RuntimeExportKind::Global,
                            "value",
                        )),
                        ImportKind::Global,
                    )],
                    Vec::new(),
                    0,
                ),
            ),
            node(
                target,
                "target.gfs",
                module(
                    Vec::new(),
                    vec![ExportSlot {
                        symbol_name: "value".to_string(),
                        kind: ExportKind::Global(GlobalIdx(0)),
                    }],
                    1,
                ),
            ),
        ],
        vec![ImportEdge {
            from: importer,
            to: target,
        }],
    )
    .expect("valid direct global graph");

    assert_eq!(
        graph
            .resolve_imports(importer)
            .expect("direct import resolves")
            .imports,
        vec![ResolvedImport {
            slot: 0,
            module_id: target,
            kind: ResolvedImportKind::Global(GlobalIdx(0)),
        }]
    );
}

#[test]
fn direct_resolution_rejects_an_import_slot_without_target_ids() {
    let importer = ModuleId::new(7);
    let target = ModuleId::new(31);
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![
            node(
                importer,
                "importer.gfs",
                module(
                    vec![import(None, None, ImportKind::Function)],
                    Vec::new(),
                    0,
                ),
            ),
            node(
                target,
                "target.gfs",
                module(
                    Vec::new(),
                    vec![ExportSlot {
                        symbol_name: "value".to_string(),
                        kind: ExportKind::Function(FuncIdx(0)),
                    }],
                    0,
                ),
            ),
        ],
        vec![ImportEdge {
            from: importer,
            to: target,
        }],
    )
    .expect("valid graph");

    assert!(matches!(
        graph.resolve_imports(importer),
        Err(GraphResolutionError::MissingDirectImportTarget {
            importer: found_importer,
            slot: 0,
        }) if found_importer == importer
    ));
}
