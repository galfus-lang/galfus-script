# Galfus Core System

This directory documents the base language system of Galfus Script.

The goal of these documents is twofold:

1. Teach how Galfus Script source code is written and how it behaves.
2. Define implementation contracts that the compiler, checker, lowering pipeline, and runtime must preserve.

Galfus is designed around deterministic compilation, explicit surfaces, compact syntax, static typing, explicit ownership behavior, and small runtime targets.

## Reading Order

1. [Reserved Words and Lexical Rules](./01-reserved-words-and-lexical-rules.md)
2. [Source, Names and Modules](./02-source-names-and-modules.md)
3. [Bindings, Literals and Primitive Types](./03-bindings-literals-and-primitive-types.md)
4. [Type System, Inference and Propagation](./04-type-system-inference-and-propagation.md)
5. [Generics](./05-generics.md)
6. [Constraints as Traits](./06-constraints-as-traits.md)
7. [Data Forms](./07-data-forms.md)
8. [Expressions and Operators](./08-expressions-and-operators.md)
9. [Mutation, Assignment and Ownership](./09-mutation-assignment-and-ownership.md)
10. [Functions and Calls](./10-functions-and-calls.md)
11. [Control Flow](./11-control-flow.md)
12. [Iteration and Ranges](./12-iteration-and-ranges.md)
13. [Pattern Matching and Narrowing](./13-pattern-matching-and-narrowing.md)
14. [Decorators and Keyword Metadata](./14-decorators-and-keyword-metadata.md)
15. [Lowering and Runtime Semantics](./15-lowering-and-runtime-semantics.md)

## Contract Language

These documents use the following contract words:

- **MUST**: required behavior.
- **MUST NOT**: forbidden behavior.
- **SHOULD**: recommended behavior unless there is a strong implementation reason to differ.
- **MAY**: allowed behavior.

## Core Invariants

Galfus Script MUST preserve these invariants:

- Source resolution is deterministic.
- Imports are explicit.
- Function return types are explicit.
- Operators have fixed meanings and are not overloaded.
- Complex assignment does not deep-copy.
- Deep copy is explicit through `copy`.
- `instanceof` narrows values and `typeof` dispatches over types.
- `for` and function parameters create constant bindings by default.
- Keyword metadata is not the same thing as decorators.
- Decorators are typed transformer functions.
- Bytecode is currently in-memory only; serialization and final bundles are
  planned, not implemented.

---

Next: [Reserved Words and Lexical Rules](./01-reserved-words-and-lexical-rules.md)
