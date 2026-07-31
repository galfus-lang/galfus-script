#[test]
fn test_mir_builder_phase3() {
    let source_id = SourceId::new(0);
    let code = r#"
        struct Point {
            x: i32,
            y: i32 = 42,
        }

        struct Point3D {
            ...Point,
            z: i32,
        }

        choice Shape {
            Circle(i32),
            Rect(i32, i32),
            Point,
        }

        fn test_structs_arrays_tuples(p: Point): i32 {
            var p1 = new(Point) { x: 10, y: 20 };
            var p2 = new(Point) { x: 5 };
            var p3 = new(Point3D) { ...p1, z: 100 };

            var arr1 = [1, 2, 3];
            var arr2 = [...arr1, 4, 5];

            var t1 = (10, 20, 30);

            var x_val = p1.x;
            var arr_val = arr2[1];

            return x_val
        }

        fn test_matches(s: Shape): i32 {
            return match s {
                Shape::Circle(r) => r,
                Shape::Rect(w, h) => w + h,
                Shape::Point => 0,
            }
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

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();

    assert!(mir_module.functions.len() >= 2);
    // Let's check that test_structs_arrays_tuples lowered successfully
    let test_func = mir_module
        .functions
        .iter()
        .find(|f| f.name == "test_structs_arrays_tuples")
        .unwrap();

    // We expect NewStruct, NewArray, NewTuple, MemberAccess, ArrayIndex in test_structs_arrays_tuples
    // Let's inspect the statements or block
    let mut found_new_struct = 0;
    let mut found_new_array = 0;
    let mut found_new_tuple = 0;
    let mut found_member_access = 0;
    let mut found_array_index = 0;

    for bb in &test_func.blocks {
        for (inst, _) in &bb.instructions {
            if let Instruction::Assign(_, rval) = inst {
                match rval {
                    RValue::NewStruct { .. } => found_new_struct += 1,
                    RValue::NewArray(..) | RValue::NewArrayDynamic(..) => found_new_array += 1,
                    RValue::NewTuple(..) => found_new_tuple += 1,
                    RValue::MemberAccess(..) => found_member_access += 1,
                    RValue::ArrayIndex(..) => found_array_index += 1,
                    _ => {}
                }
            }
        }
    }

    assert!(
        found_new_struct >= 3,
        "Expected at least 3 struct instantiations"
    );
    assert!(
        found_new_array >= 2,
        "Expected at least 2 array instantiations"
    );
    assert!(
        found_new_tuple >= 1,
        "Expected at least 1 tuple instantiation"
    );
    assert!(
        found_member_access >= 1,
        "Expected at least 1 member accesses"
    ); // p1.x
    assert!(
        found_array_index >= 1,
        "Expected at least 1 array index access"
    );

    let match_func = mir_module
        .functions
        .iter()
        .find(|f| f.name == "test_matches")
        .unwrap();
    // Verify that match_func is built with branches (more than 1 block)
    assert!(
        match_func.blocks.len() > 1,
        "Expected match expression to generate multiple blocks"
    );
}
