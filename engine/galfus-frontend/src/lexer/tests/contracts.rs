use super::*;

#[test]
fn batch_lexing_matches_the_streaming_lexer() {
    let source = source("value = typeof\n¬");
    let batch = lex(&source);

    let mut lexer = Lexer::new(&source);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        let is_eof = token.kind() == &TokenKind::Eof;
        tokens.push(token);

        if is_eof {
            break;
        }
    }

    assert_eq!(batch.tokens(), tokens);
    assert_eq!(batch.diagnostics().len(), lexer.diagnostics().len());
}
