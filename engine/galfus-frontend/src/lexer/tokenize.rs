use super::*;

impl Lexer<'_> {
    pub fn next_token(&mut self) -> Token {
        self.skip_trivia();

        let start = self.offset;

        if self.is_eof() {
            return self.make_token(TokenKind::Eof, Span::empty(self.source.id(), start));
        }

        let ch = self.bump().expect("lexer offset should not be EOF");

        if Self::is_identifier_start(ch) {
            let kind = self.lex_identifier(start);
            return self.make_token(kind, Span::new(self.source.id(), start, self.offset));
        }

        if Self::is_decimal_digit(ch) {
            let kind = self.lex_number(start);
            return self.make_token(kind, Span::new(self.source.id(), start, self.offset));
        }

        if ch == '"' || ch == '\'' {
            let kind = self.lex_string(start, ch);
            return self.make_token(kind, Span::new(self.source.id(), start, self.offset));
        }

        if ch == '`' {
            let kind = self.lex_multiline_string(start);
            return self.make_token(kind, Span::new(self.source.id(), start, self.offset));
        }

        let kind = match ch {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,

            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '@' => TokenKind::At,

            '.' => {
                if self.match_char('.') {
                    if self.match_char('.') {
                        TokenKind::DotDotDot
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }

            ':' => {
                if self.match_char(':') {
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }

            '?' => {
                if self.match_char('?') {
                    if self.match_char('=') {
                        TokenKind::QuestionQuestionEqual
                    } else {
                        TokenKind::QuestionQuestion
                    }
                } else if self.match_char('.') {
                    TokenKind::QuestionDot
                } else {
                    TokenKind::Unknown
                }
            }

            '+' => {
                if self.match_char('=') {
                    TokenKind::PlusEqual
                } else {
                    TokenKind::Plus
                }
            }

            '-' => {
                if self.match_char('=') {
                    TokenKind::MinusEqual
                } else {
                    TokenKind::Minus
                }
            }

            '*' => {
                if self.match_char('*') {
                    if self.match_char('=') {
                        TokenKind::StarStarEqual
                    } else {
                        TokenKind::StarStar
                    }
                } else if self.match_char('=') {
                    TokenKind::StarEqual
                } else {
                    TokenKind::Star
                }
            }

            '/' => {
                if self.match_char('=') {
                    TokenKind::SlashEqual
                } else {
                    TokenKind::Slash
                }
            }

            '%' => {
                if self.match_char('=') {
                    TokenKind::PercentEqual
                } else {
                    TokenKind::Percent
                }
            }

            '!' => {
                if self.match_char('=') {
                    TokenKind::BangEqual
                } else {
                    TokenKind::Bang
                }
            }

            '=' => {
                if self.match_char('=') {
                    TokenKind::EqualEqual
                } else if self.match_char('>') {
                    TokenKind::Arrow
                } else {
                    TokenKind::Equal
                }
            }

            '<' => {
                if self.match_char('<') {
                    if self.match_char('=') {
                        TokenKind::ShiftLeftEqual
                    } else {
                        TokenKind::ShiftLeft
                    }
                } else if self.match_char('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }

            '>' => {
                if self.match_char('>') {
                    if self.match_char('=') {
                        TokenKind::ShiftRightEqual
                    } else {
                        TokenKind::ShiftRight
                    }
                } else if self.match_char('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }

            '&' => {
                if self.match_char('&') {
                    TokenKind::AmpAmp
                } else if self.match_char('=') {
                    TokenKind::AmpEqual
                } else {
                    TokenKind::Amp
                }
            }

            '|' => {
                if self.match_char('|') {
                    TokenKind::PipePipe
                } else if self.match_char('=') {
                    TokenKind::PipeEqual
                } else {
                    TokenKind::Pipe
                }
            }

            '^' => {
                if self.match_char('=') {
                    TokenKind::CaretEqual
                } else {
                    TokenKind::Caret
                }
            }

            '~' => TokenKind::Tilde,

            '\n' => TokenKind::Newline,
            '\r' => {
                if self.peek() == Some('\n') {
                    self.bump();
                }

                TokenKind::Newline
            }

            _ => {
                let span = Span::new(self.source.id(), start, self.offset);

                self.diagnostics.push(Diagnostic::error(
                    LexicalDiagnosticCode::UnknownCharacter,
                    span,
                ));

                TokenKind::Unknown
            }
        };

        self.make_token(kind, Span::new(self.source.id(), start, self.offset))
    }
}
