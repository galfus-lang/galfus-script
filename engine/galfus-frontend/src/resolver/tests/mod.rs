mod block;
mod export;
mod function;
mod hardening;
mod import;
mod reference;
mod scope;
mod symbol;
mod type_member;
mod type_reference;

use crate::{GraphPhase, ScopeKind, SymbolKind, SyntaxNodeKind, parse, resolve};
use galfus_core::{SourceFile, SourceId};

fn source(text: &str) -> SourceFile {
    SourceFile::new(SourceId::new(0), "main.gfs".to_string(), text.to_string())
}
