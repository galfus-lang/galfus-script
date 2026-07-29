use crate::semantic_to_mir::*;
use galfus_core::FunctionId;
use galfus_core::{SourceFile, SourceId};
use galfus_frontend::{check_declaration_types, check_definition_types, parse, resolve};
use galfus_ir::mir::*;

include!("builder/lowering.rs");
include!("builder/phases.rs");
