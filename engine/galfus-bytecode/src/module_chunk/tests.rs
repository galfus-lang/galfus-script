use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath, SemanticRevision};

use super::{
    ModuleChunk, ModuleChunkDecodingError, ModuleChunkValidationError, canonical_node_content_hash,
};
use crate::{
    BytecodeModule, BytecodeNode, CURRENT_BYTECODE_FORMAT_VERSION, ConstantPool, ModuleDescriptor,
    instruction,
};

fn node(module_id: ModuleId) -> BytecodeNode {
    BytecodeNode {
        id: module_id,
        path: ModulePath::new(format!("src/{}.gfs", module_id.raw()).as_str())
            .expect("valid module path"),
        semantic_revision: SemanticRevision::new(17),
        module: BytecodeModule {
            name: format!("module-{}", module_id.raw()),
            global_count: 0,
            constants: ConstantPool::default(),
            functions: Vec::new(),
            types: Vec::new(),
            struct_layouts: Vec::new(),
            choice_layouts: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            init_func_idx: None,
        },
        metadata: None,
    }
}

fn descriptor(node: &BytecodeNode) -> ModuleDescriptor {
    ModuleDescriptor::new(
        node.id(),
        node.path().clone(),
        Vec::new(),
        Vec::new(),
        false,
        ContentHash::of(b"interface"),
        canonical_node_content_hash(node).expect("node encodes"),
    )
    .expect("descriptor is valid")
}

fn chunk(module_id: ModuleId) -> (ModuleChunk, ModuleDescriptor) {
    let node = node(module_id);
    let descriptor = descriptor(&node);
    let chunk = ModuleChunk::from_node(CURRENT_BYTECODE_FORMAT_VERSION, node, &descriptor)
        .expect("chunk is valid");
    (chunk, descriptor)
}

#[test]
fn chunk_round_trips_through_canonical_bytes() {
    let (chunk, descriptor) = chunk(ModuleId::new(7));

    let encoded = chunk.canonical_bytes().expect("chunk encodes");
    let decoded = ModuleChunk::from_bytecode(encoded.as_slice()).expect("chunk decodes");

    assert_eq!(decoded, chunk);
    decoded.verify(&descriptor).expect("chunk verifies");
    assert_eq!(
        decoded.node().semantic_revision(),
        SemanticRevision::default()
    );
}

#[test]
fn chunk_rejects_a_wrong_owner_module_id() {
    let (mut chunk, descriptor) = chunk(ModuleId::new(7));
    chunk.module_id = ModuleId::new(19);

    assert!(matches!(
        chunk.verify(&descriptor),
        Err(ModuleChunkValidationError::NodeModuleIdMismatch { owner, node })
            if owner == ModuleId::new(19) && node == ModuleId::new(7)
    ));
}

#[test]
fn chunk_rejects_altered_bytecode_content() {
    let (mut chunk, descriptor) = chunk(ModuleId::new(7));
    chunk.node.module.name.push_str("-altered");
    let encoded = postcard::to_stdvec(&chunk).expect("tampered chunk encodes");

    assert!(matches!(
        ModuleChunk::from_bytecode(encoded.as_slice()),
        Err(ModuleChunkDecodingError::Validation(
            ModuleChunkValidationError::ContentHashMismatch { module_id, .. }
        ))
            if module_id == ModuleId::new(7)
    ));
    assert!(matches!(
        chunk.verify(&descriptor),
        Err(ModuleChunkValidationError::ContentHashMismatch { module_id, .. })
            if module_id == ModuleId::new(7)
    ));
}

#[test]
fn chunk_rejects_an_altered_interface_hash() {
    let (mut chunk, descriptor) = chunk(ModuleId::new(7));
    chunk.interface_hash = ContentHash::of(b"altered interface");

    assert!(matches!(
        chunk.verify(&descriptor),
        Err(ModuleChunkValidationError::InterfaceHashMismatch { module_id, .. })
            if module_id == ModuleId::new(7)
    ));
}

#[test]
fn chunk_rejects_an_invalid_node() {
    let mut node = node(ModuleId::new(7));
    node.module.init_func_idx = Some(instruction::FuncIdx(0));
    let descriptor = descriptor(&node);

    assert!(matches!(
        ModuleChunk::from_node(CURRENT_BYTECODE_FORMAT_VERSION, node, &descriptor),
        Err(ModuleChunkValidationError::InvalidBytecode { module_id, .. })
            if module_id == ModuleId::new(7)
    ));
}

#[test]
fn chunk_decoder_rejects_trailing_bytes() {
    let (chunk, _) = chunk(ModuleId::new(7));
    let mut encoded = chunk.canonical_bytes().expect("chunk encodes");
    encoded.push(0);

    assert!(matches!(
        ModuleChunk::from_bytecode(encoded.as_slice()),
        Err(ModuleChunkDecodingError::UnexpectedTrailingBytes)
    ));
}
