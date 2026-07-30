# Instructions for Agents

## Project Configuration

- **Languages**: Rust (Core), TypeScript / Bun (CI/CD and Automation Scripts)

## 1) Priority and Scope

Follow these instructions in this order:

1. Correctness and safety.
2. Exact user intent.
3. Minimal task completion.
4. Clarity and concision.
5. Token efficiency.

The agent must implement only what the user explicitly requested.

When the requested task is ambiguous, choose the narrowest reasonable interpretation. If the ambiguity can cause incorrect changes, ask for clarification before modifying files.

## 2) Strict Scope Control

The agent must not expand the task scope on its own.

Do not implement features, fixes, refactors, cleanups, optimizations, test rewrites, documentation updates, or architecture changes unless they are explicitly requested or strictly necessary to complete the requested task.

If an unrelated issue is found, report it separately under **"Found but not changed"** and do not modify it.

Examples:

- If the user asks to fix a parser bug, do not refactor the lexer.
- If the user asks to update documentation, do not change runtime code.
- If the user asks for analysis only, do not modify files.
- If tests reveal unrelated failures, report them; do not fix them unless requested.
- If an unused function is discovered outside the requested change, report it; do not remove it.

## 3) Communication Rules

- Keep responses short and concise whenever possible.
- Use the user's language for conversation.
- Write all code, comments, commit messages, and technical documentation in English.
- Do not blindly approve proposals. Evaluate them logically, explain trade-offs, and suggest a better alternative when needed.
- Surface blockers, assumptions, and risks early.
- Clearly separate:
  - **Changed**
  - **Not changed**
  - **Found but not changed**
  - **Validation**

## 4) Pre-Implementation Self-Assessment

Before generating or modifying any code, the agent must ask and answer these questions internally:

1. Is this code necessary to satisfy the exact user request?
2. Is this change inside the requested scope?
3. Does a function, utility, or pattern already exist in the codebase that solves this?
4. Does the standard library or a well-maintained published library solve this problem?
5. Are any dependencies required?

If the answer indicates the change is outside scope, do not implement it. Report it instead.

The final code must be minimal, efficient, and focused solely on completing the requested task.

## 5) Planning Rules

Create a brief strategic plan before implementation only when the task requires code changes, file changes, or multi-step analysis.

Plans must be brief and actionable:

- Scope
- Files likely involved
- Risks
- Validation

If the user asks only for analysis, produce analysis only.

Do not create implementation plans for unrelated improvements.

## 6) Implementation Rules

- Prefer minimal, high-impact changes over broad refactors.
- Reuse existing utilities and project patterns before adding new abstractions.
- Do not perform drive-by refactors.
- Do not reformat unrelated files.
- Do not rename symbols unless required by the requested task.
- Do not change public APIs unless explicitly requested or strictly required.
- Do not add new dependencies unless explicitly requested or clearly necessary for the requested task.
- Do not create new scripts unless explicitly requested.
- If repetitive work is detected, propose automation, but do not implement the automation unless the user asks for it.
- Only remove unused variables, imports, or functions introduced by the agent's own changes.
- Do not remove pre-existing unused variables, imports, or functions unless the user explicitly asked for cleanup.
- Target source file size around 300 lines; avoid exceeding 600 lines when splitting is practical. This does not apply to `.md` documentation files.
- If files are changed externally while the agent works, treat those changes as intentional and do not revert them unless explicitly requested.

## 7) Temporary Files

Files inside `.tmp/` are temporary and may be used for planning, prototyping, and testing.

Rules:

- Temporary files must be placed in `.tmp/` at the project root.
- Temporary files must not be used for permanent code or documentation.
- Temporary files may be modified or deleted without warning.
- Do not create temporary files outside `.tmp/`.

## 8) Validation and Uncertainty Rules

When uncertain about framework or API usage, consult official documentation before implementing.

Before concluding a code-changing task, validate only what is relevant to the requested change.

Available validation commands:

### Rust

- Build/Check: `cargo check`
- Formatter: `cargo fmt --check`
- Linter: `cargo clippy --workspace --all-targets`
- Tests: `cargo test --workspace`

Note: running `cargo test` alone only runs CLI tests. Use `cargo test --workspace` when full workspace validation is required.

### TypeScript / Bun

- Execution/Validation: `bun run <script_path>`

Validation rules:

- Do not fix unrelated validation failures.
- If unrelated failures appear, report them under **"Found but not changed"**.
- If full validation is expensive or unnecessary, run the smallest relevant validation and explain what was not run.
- If validation cannot be run, explain why.

## 9) Project Rules

- Variables that hold ownership and are no longer used afterward must receive the `_` prefix.

  Example:

  ```rust
  let _guard = lock.write();
  ```

- If variables are unused and do not hold ownership or side effects, remove them only when they were introduced by the current change.

- Remove unused functions only when they were introduced by the current change.

- The project does not use a garbage collector.

- The project uses an ownership graph, where resource release follows a path or trail through the graph rather than a garbage collector cycle.

- Internal Rust properties use `snake_case`.

- Rust source files must be organized in the following order:
  1. `mod` declarations
  2. `use` imports
  3. Rest of the code

- Do not use explicit paths in the middle of code.

  Avoid:

  ```rust
  crate::module1::module2::ElementUsed
  ```

  Prefer importing at the top of the file with `use`.

- If there are tests, they must be in a separate file.

- The test module declaration must be placed at the top of the parent file:

  ```rust
  #[cfg(test)]
  mod tests;

  // Rest of the file
  ```

## 10) Tests

Do not add, rewrite, or delete tests unless one of these is true:

1. The user explicitly asked for tests.
2. The requested code change requires a minimal test to prove the requested behavior.
3. Existing tests must be adjusted because the requested behavior intentionally changed.

Do not update unrelated tests.

If unrelated tests fail, report them but do not modify them.

## 11) Documentation

Do not update documentation unless one of these is true:

1. The user explicitly asked for documentation changes.
2. The requested task is documentation-related.
3. The requested code change makes existing documentation directly incorrect.

If documentation problems are found outside the requested task, report them under **"Found but not changed"**.

## 12) Output Format

For implementation tasks, the final response should include:

- **Changed**: what was modified.
- **Why**: why the change was necessary.
- **Validation**: what was run and the result.
- **Found but not changed**: unrelated issues discovered, if any.

For analysis-only tasks, the final response should include:

- **Findings**
- **Risks**
- **Suggested next steps**

Do not claim that unrelated issues were fixed unless the user explicitly requested those fixes.
