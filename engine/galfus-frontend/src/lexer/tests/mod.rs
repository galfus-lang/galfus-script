mod contracts;
mod core;
mod identifiers;
mod numbers;
mod result;
mod strings;
mod trivia;

use super::*;
use galfus_core::{SourceFile, SourceId, Span};

fn source(text: &str) -> SourceFile {
    SourceFile::new(SourceId::new(0), "main.gfs".to_string(), text.to_string())
}

fn kinds(text: &str) -> Vec<TokenKind> {
    let source = source(text);
    let mut lexer = Lexer::new(&source);

    let mut kinds = Vec::new();

    loop {
        let token = lexer.next_token();
        let kind = token.kind().clone();

        kinds.push(kind.clone());

        if kind == TokenKind::Eof {
            break;
        }
    }

    kinds
}
