mod cursor;
mod identifier;
mod numbers;
mod result;
mod state;
mod strings;
#[cfg(test)]
mod tests;
mod tokenize;
mod trivia;

use crate::{LexicalDiagnosticCode, Token, TokenKind};
use galfus_core::{Diagnostic, Span};
pub use result::LexResult;
pub use state::{Lexer, lex};
