use crate::bytecode_emission::*;
use crate::semantic_to_mir::*;
use galfus_core::{SourceFile, SourceId};
use galfus_frontend::{check_declaration_types, check_definition_types, parse, resolve};
use galfus_ir::mir::*;

#[test]
fn test_mir_builder_phase4() {
    let source_id = SourceId::new(0);
    let code = r#"
        var g_var = 100
        const g_const = "global_const"

        struct Point {
            x: i32,
            y: i32,
        }

        fn test_drops(cond: bool): i32 {
            var pt = new(Point) { x: 10, y: 20 }
            if cond {
                var pt2 = new(Point) { x: 30, y: 40 }
                return pt2.x
            }
            return pt.x
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(
        !graph.has_errors(),
        "Parse or resolve errors occurred: {:?}",
        graph.diagnostics()
    );

    let type_result =
        check_definition_types(&source, &graph, check_declaration_types(&source, &graph));
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();

    // Verify globals: g_var and g_const
    assert_eq!(mir_module.globals.len(), 2);
    let g_var = mir_module
        .globals
        .iter()
        .find(|g| g.name == "g_var")
        .unwrap();
    let g_const = mir_module
        .globals
        .iter()
        .find(|g| g.name == "g_const")
        .unwrap();
    assert_eq!(g_var.name, "g_var");
    assert_eq!(g_const.name, "g_const");

    // Verify __init_module function is built
    let init_func = mir_module
        .functions
        .iter()
        .find(|f| f.name == "__init_module")
        .unwrap();
    assert_eq!(init_func.name, "__init_module");

    // Verify test_drops contains Drop instructions
    let drops_func = mir_module
        .functions
        .iter()
        .find(|f| f.name == "test_drops")
        .unwrap();

    let mut found_drops = 0;
    fn count_drops(func: &MirFunction, found_drops: &mut usize) {
        for block in &func.blocks {
            for (inst, _) in &block.instructions {
                if matches!(inst, Instruction::Drop(_)) {
                    *found_drops += 1;
                }
            }
        }
    }
    count_drops(drops_func, &mut found_drops);
    assert!(
        found_drops > 0,
        "Expected at least one Drop instruction in test_drops"
    );

    // Verify validator accepts the module
    let validation = galfus_ir::validator::validate_module(&mir_module);
    assert!(
        validation.is_ok(),
        "Expected validation to succeed, but found errors: {:?}",
        validation.err()
    );
}

#[test]
fn test_mir_lowering_basic() {
    let source_id = SourceId::new(0);
    let code = r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn compute(a: i32, b: i32): i32 {
            var pt = new(Point) { x: a, y: b };
            return pt.x + pt.y
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(
        !graph.has_errors(),
        "Parse/Resolve error: {:?}",
        graph.diagnostics()
    );

    let type_result =
        check_definition_types(&source, &graph, check_declaration_types(&source, &graph));
    assert!(
        !type_result.has_errors(),
        "Typecheck error: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let (module_image, _) = lower_module(&mir_module, &type_result, &graph, code);

    // Verify bytecode module metadata
    assert!(!module_image.functions.is_empty());
    let compute_func = module_image
        .functions
        .iter()
        .find(|f| f.name == "compute")
        .expect("compute function not found");

    assert_eq!(compute_func.param_count, 2);
    // locals: pt + MIR temporaries
    assert!(!compute_func.instructions.is_empty());

    // Verify struct layout was created
    assert!(!module_image.struct_layouts.is_empty());
    let pt_layout = &module_image.struct_layouts[0];
    assert_eq!(pt_layout.name, "Point");
    assert_eq!(pt_layout.fields.len(), 2);
    assert_eq!(pt_layout.fields[0].name, "x");
    assert_eq!(pt_layout.fields[1].name, "y");
}

#[test]
fn test_mir_lowering_defaults_integer_constants_to_int32() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn main(): i32 {
            return 42
        }
    "#;
    let source = SourceFile::new(
        source_id,
        "test_int_default.gfs".to_string(),
        code.to_string(),
    );

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(
        !graph.has_errors(),
        "Parse/Resolve error: {:?}",
        graph.diagnostics()
    );

    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck error: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let (module_image, _) = lower_module(&mir_module, &type_result, &graph, code);

    assert!(
        module_image
            .constants
            .constants
            .iter()
            .any(|constant| matches!(constant, galfus_bytecode::Constant::Int32(42)))
    );
}

#[test]
fn test_mir_lowering_advanced() {
    let source_id = SourceId::new(0);
    let code = r#"
        choice Shape {
            Circle(i32),
            Square,
        }

        fn process(s: Shape): i32 {
            return match s {
                Shape::Circle(r) => r * r,
                Shape::Square => 0,
            }
        }

        fn calculate_sum(limit: i32): i32 {
            var sum = 0;
            var i = 0;
            loop {
                if i >= limit {
                    break;
                }
                if i == 5 {
                    i = i + 1;
                    continue;
                }
                sum = sum + i;
                i = i + 1;
            }
            return sum;
        }

        fn tuple_operations(): (i32, i32) {
            var t = (10, 20);
            return t;
        }
    "#;
    let source = SourceFile::new(source_id, "test_adv.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(
        !graph.has_errors(),
        "Parse/Resolve error: {:?}",
        graph.diagnostics()
    );

    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck error: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let (module_image, _) = lower_module(&mir_module, &type_result, &graph, code);

    // Verify functions
    assert!(!module_image.functions.is_empty());

    // 1. process (choice match, returns, expression)
    let process_func = module_image
        .functions
        .iter()
        .find(|f| f.name == "process")
        .expect("process func not found");
    assert_eq!(process_func.param_count, 1);
    assert!(!process_func.instructions.is_empty());

    // 2. calculate_sum (loop, if, break, continue, comparison)
    let sum_func = module_image
        .functions
        .iter()
        .find(|f| f.name == "calculate_sum")
        .expect("calculate_sum func not found");
    assert_eq!(sum_func.param_count, 1);
    assert!(!sum_func.instructions.is_empty());

    // 3. tuple_operations (tuple construction)
    let tuple_func = module_image
        .functions
        .iter()
        .find(|f| f.name == "tuple_operations")
        .expect("tuple_operations func not found");
    assert_eq!(tuple_func.param_count, 0);
    assert!(!tuple_func.instructions.is_empty());

    // Verify choice layout was compiled
    assert!(!module_image.choice_layouts.is_empty());
    let shape_layout = &module_image.choice_layouts[0];
    assert_eq!(shape_layout.name, "Shape");
    assert_eq!(shape_layout.variants.len(), 2);
    assert_eq!(shape_layout.variants[0].name, "Circle");
    assert_eq!(shape_layout.variants[1].name, "Square");
}

#[test]
fn test_mir_builder_for_loop() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn test_for(): i32 {
            var sum = 0;
            for i in 0..10 {
                sum = sum + i;
            }
            return sum;
        }
    "#;
    let source = SourceFile::new(source_id, "test_for.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(
        !graph.has_errors(),
        "Parse/Resolve error: {:?}",
        graph.diagnostics()
    );

    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck error: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();

    assert_eq!(mir_module.functions.len(), 1);
    let func = &mir_module.functions[0];
    assert_eq!(func.name, "test_for");

    // Check that the body contains the loop blocks.
    assert!(
        func.blocks.len() > 1,
        "Expected for loop to lower to multiple blocks"
    );
}

#[test]
fn test_async_call_emits_typed_future_instruction() {
    let source_id = SourceId::new(0);
    let code = r#"
        struct Future<T> { id: i64 }

        fn(async) load(value: i32): i32 {
            return value
        }

        fn(async) main(): i32 {
            const future = load(7)
            return await future
        }
    "#;
    let source = SourceFile::new(source_id, "typed_future.gfs".to_string(), code.to_string());
    let parse_result = parse(&source);
    let graph = resolve(&source, parse_result.into_graph()).into_graph();
    let type_result =
        check_definition_types(&source, &graph, check_declaration_types(&source, &graph));
    assert!(
        !type_result.has_errors(),
        "Typecheck error: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let (module, _) = lower_module(&mir_module, &type_result, &graph, code);
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function should be emitted");
    let instruction = main
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            galfus_bytecode::Instruction::CreateFuture {
                arg_count,
                arg_types,
                return_type,
                ..
            } => Some((*arg_count, arg_types, *return_type)),
            _ => None,
        })
        .expect("async call should emit CreateFuture");

    assert_eq!(instruction.0, 1);
    assert_eq!(instruction.1.len(), 1);
    assert!(matches!(
        module.types[instruction.2.raw() as usize],
        galfus_bytecode::BytecodeType::Int32
    ));
}

#[test]
fn test_indirect_async_call_emits_typed_future_instruction() {
    let source_id = SourceId::new(0);
    let code = r#"
        struct Future<T> { id: i64 }

        fn(async) load(value: i32): i32 {
            return value
        }

        fn(async) main(): i32 {
            const callback = load
            const future = callback(7)
            return await future
        }
    "#;
    let source = SourceFile::new(
        source_id,
        "indirect_future.gfs".to_string(),
        code.to_string(),
    );
    let parse_result = parse(&source);
    let graph = resolve(&source, parse_result.into_graph()).into_graph();
    let type_result =
        check_definition_types(&source, &graph, check_declaration_types(&source, &graph));
    assert!(
        !type_result.has_errors(),
        "Typecheck error: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let (module, _) = lower_module(&mir_module, &type_result, &graph, code);
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function should be emitted");
    let instruction = main
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            galfus_bytecode::Instruction::CreateIndirectFuture {
                arg_count,
                arg_types,
                return_type,
                ..
            } => Some((*arg_count, arg_types, *return_type)),
            _ => None,
        })
        .expect("indirect async call should emit CreateIndirectFuture");

    assert_eq!(instruction.0, 1);
    assert_eq!(instruction.1.len(), 1);
    assert!(matches!(
        module.types[instruction.2.raw() as usize],
        galfus_bytecode::BytecodeType::Int32
    ));
}
