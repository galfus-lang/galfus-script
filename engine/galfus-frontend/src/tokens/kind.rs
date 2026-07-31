#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Special
    Eof,
    Newline,
    Unknown,

    // Literals / identifiers
    Identifier,
    Integer,
    Float,
    String,

    // Keywords
    Import,
    From,
    Export,
    As,
    Var,
    Const,
    Fn,
    Return,
    Struct,
    Enum,
    Choice,
    Type,
    Constraint,
    Satisfies,
    Match,
    Instanceof,
    Typeof,
    If,
    Else,
    For,
    In,
    Loop,
    Break,
    Continue,
    Weak,
    Null,
    True,
    False,
    New,
    Copy,

    SelfKw,

    // Delimiters
    LeftParen,    // (
    RightParen,   // )
    LeftBrace,    // {
    RightBrace,   // }
    LeftBracket,  // [
    RightBracket, // ]

    // Punctuation
    Comma,      // ,
    Dot,        // .
    Colon,      // :
    Semicolon,  // ;
    At,         // @
    Underscore, // _

    // Operators
    Plus,     // +
    Minus,    // -
    Star,     // *
    Slash,    // /
    Percent,  // %
    StarStar, // **

    Bang,       // !
    Equal,      // =
    EqualEqual, // ==
    BangEqual,  // !=

    Less,         // <
    LessEqual,    // <=
    Greater,      // >
    GreaterEqual, // >=

    Amp,   // &
    Pipe,  // |
    Caret, // ^
    Tilde, // ~

    AmpAmp,   // &&
    PipePipe, // ||

    ShiftLeft,  // <<
    ShiftRight, // >>

    PlusEqual,     // +=
    MinusEqual,    // -=
    StarEqual,     // *=
    SlashEqual,    // /=
    PercentEqual,  // %=
    StarStarEqual, // **=

    AmpEqual,        // &=
    PipeEqual,       // |=
    CaretEqual,      // ^=
    ShiftLeftEqual,  // <<=
    ShiftRightEqual, // >>=

    QuestionDot,           // ?.
    QuestionQuestion,      // ??
    QuestionQuestionEqual, // ??=

    ColonColon, // ::
    DotDot,     // ..
    DotDotDot,  // ...
    Arrow,      // =>
}
