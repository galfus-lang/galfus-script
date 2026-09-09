use crate::{
    ImportedType, PrimitiveType, SymbolKind, build_module_surface, check_declaration_types, parse,
    resolve,
};
use galfus_core::{SourceFile, SourceId, SymbolId};

fn source(text: &str) -> SourceFile {
    SourceFile::new(SourceId::new(0), "main.gfs".to_string(), text.to_string())
}

#[test]
fn module_surface_records_exported_type_definitions() {
    let source = source(
        r#"
        export type UserId = i64

        export struct User {
            id: UserId,
        }

        export enum Status {
            Active,
        }

        struct PrivateUser {
            id: UserId,
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let type_result = check_declaration_types(&source, graph, &string_table, false);
    assert!(!type_result.has_errors());

    let surface = build_module_surface(
        galfus_core::ModuleId::new(1),
        &source,
        graph,
        &type_result,
        &string_table,
    );

    assert_eq!(surface.exports().len(), 3);
    assert_eq!(
        surface.export("UserId").unwrap().kind(),
        SymbolKind::TypeAlias
    );
    assert_eq!(surface.export("User").unwrap().kind(), SymbolKind::Struct);
    assert_eq!(surface.export("Status").unwrap().kind(), SymbolKind::Enum);
    assert!(surface.export("PrivateUser").is_none());
    assert!(surface.export("User").unwrap().ty().is_some());
}

#[test]
fn module_surface_imports_exported_type_as_local_binding() {
    let source = source(
        r#"
        export struct User {
            id: i64,
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let type_result = check_declaration_types(&source, graph, &string_table, false);
    assert!(!type_result.has_errors());

    let surface = build_module_surface(
        galfus_core::ModuleId::new(1),
        &source,
        graph,
        &type_result,
        &string_table,
    );
    let local_symbol = SymbolId::new(42);

    assert_eq!(
        surface.imported_type_for_export(local_symbol, "User"),
        Some(ImportedType::NamedLocal {
            symbol: local_symbol
        })
    );
}

#[test]
fn module_surface_imports_exported_type_as_namespace_path() {
    let source = source(
        r#"
        export struct User {
            id: i64,
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let type_result = check_declaration_types(&source, graph, &string_table, false);
    assert!(!type_result.has_errors());

    let surface = build_module_surface(
        galfus_core::ModuleId::new(1),
        &source,
        graph,
        &type_result,
        &string_table,
    );
    let namespace = SymbolId::new(7);

    assert_eq!(
        surface.imported_path_type_for_export(namespace, "User"),
        Some(ImportedType::SurfacePath {
            namespace,
            name: "User".to_string(),
        })
    );
}

#[test]
fn module_surface_includes_exported_struct_methods() {
    let source = source(
        r#"
        export struct Stream {}

        export fn(async) Stream::next(self): i32 {
          return 0
        }

        export fn Stream::send(self, value: [u8]): bool => true
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let type_result = check_declaration_types(&source, graph, &string_table, false);
    assert!(!type_result.has_errors());

    let surface = build_module_surface(
        galfus_core::ModuleId::new(1),
        &source,
        graph,
        &type_result,
        &string_table,
    );

    let next = surface
        .export("Stream")
        .expect("Stream is exported")
        .members()
        .iter()
        .find(|member| member.name() == "next" && member.kind() == SymbolKind::Function)
        .expect("exported async method is included in the module surface");
    assert_eq!(
        next.ty(),
        Some(&ImportedType::Function {
            parameters: vec![crate::ImportedFunctionParameterType::new(
                ImportedType::LocalPath {
                    name: "Stream".to_string(),
                },
            )],
            return_type: Box::new(ImportedType::GenericInstance {
                base: Box::new(ImportedType::LocalPath {
                    name: "Future".to_string(),
                }),
                arguments: vec![ImportedType::Primitive(PrimitiveType::Int32)],
            }),
        })
    );
    assert!(
        surface
            .export("Stream")
            .expect("Stream is exported")
            .members()
            .iter()
            .any(|member| member.name() == "send" && member.kind() == SymbolKind::Function),
        "exported synchronous method is included in the module surface"
    );
}

#[test]
fn module_surface_includes_thread_methods() {
    let source = source(galfus_contract::THREAD_SOURCE);
    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let type_result = check_declaration_types(&source, graph, &string_table, false);
    assert!(!type_result.has_errors());

    let surface = build_module_surface(
        galfus_core::ModuleId::new(1),
        &source,
        graph,
        &type_result,
        &string_table,
    );
    let thread = surface.export("Thread").expect("Thread is exported");
    for method in ["spawn", "send", "receiveMessage", "wait"] {
        assert!(
            thread
                .members()
                .iter()
                .any(|member| member.name() == method && member.kind() == SymbolKind::Function),
            "Thread::{method} is exported through the module surface"
        );
    }
}

#[test]
fn module_surface_records_exported_function_signature() {
    let source = source(
        r#"
        export fn add(a: i32, b: i32 = 1): i32 {
            return a
        }

        fn helper(): null {
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let type_result = check_declaration_types(&source, graph, &string_table, false);
    assert!(!type_result.has_errors());

    let surface = build_module_surface(
        galfus_core::ModuleId::new(1),
        &source,
        graph,
        &type_result,
        &string_table,
    );
    let add = surface.export("add").unwrap();

    assert_eq!(add.kind(), SymbolKind::Function);
    assert!(surface.export("helper").is_none());

    let Some(ImportedType::Function {
        parameters,
        return_type,
    }) = add.ty()
    else {
        panic!("exported function should carry a transportable signature");
    };

    assert_eq!(parameters.len(), 2);
    assert_eq!(
        parameters[0].ty(),
        &ImportedType::Primitive(PrimitiveType::Int32)
    );
    assert!(!parameters[0].has_default());
    assert_eq!(
        parameters[1].ty(),
        &ImportedType::Primitive(PrimitiveType::Int32)
    );
    assert!(parameters[1].has_default());
    assert_eq!(
        return_type.as_ref(),
        &ImportedType::Primitive(PrimitiveType::Int32)
    );
}

#[test]
fn module_surface_preserves_choice_variants_without_payloads() {
    let source = source(
        r#"
        export choice ReadResult<T> {
            Data(T),
            End,
            Error([u8]),
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let type_result = check_declaration_types(&source, graph, &string_table, false);
    assert!(!type_result.has_errors());

    let surface = build_module_surface(
        galfus_core::ModuleId::new(1),
        &source,
        graph,
        &type_result,
        &string_table,
    );
    let choice = surface
        .export("ReadResult")
        .expect("choice must be exported")
        .imported_choice_surface(None);

    assert_eq!(choice.variants().len(), 3);
    assert_eq!(choice.variants()[1].name(), "End");
    assert!(choice.variants()[1].payload_types().is_empty());
}
