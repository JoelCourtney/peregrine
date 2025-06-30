# Claude context

I am creating a discrete event simulator called Peregrine, which is a successor to Merlin.
I'm converting the aerie-lander demo model from Java into Rust to use as a demo for Peregrine,
and I'm adding new features to Peregrine as I go.

## Bash commands

All are run in the root of the git repo.

- Run tests: `cargo test`

  Re-run a specific test with `cargo test <fn_name>` where `<fn_name>` is the
  name of the test function, without hairpins.
- Quick check for errors and warnings: `cargo check`.
- Linter: `cargo clippy --all-targets --workspace -- -D warnings`
  The CI pipeline will deny all lints.
- Format: `cargo fmt`
- Git: use the `gh` cli

Before making any commits, you MUST make sure that all tests
pass, there are no lints remaining, and run the formatter.
The CI pipeline will fail otherwise.

## Code style

- Combine imports when possible. Meaning:
  ```rust
  // GOOD
  use my_crate::{ImportA, ImportB};
  ```
  Instead of:
  ```rust
  // BAD
  use my_crate::ImportA;
  use my_crate::ImportB;
  ```
  The formatter does not do this automatically.
- When creating new git branches, name them `claude/...`

## Workflow

1. Read the relevant files. Look for usages of the relevant code if that's applicable.
2. Make a plan. It should include creating tests if relevant.
3. Implement your plan, step-by-step. After each step, run `cargo check` to catch errors
   and warnings early.
4. Make sure tests pass.
5. Fix all clippy lints and warnings. Ask for approval before silencing any lints.
6. Run `cargo fmt` after every iteration. Every time you finish an instruction, you MUST run `cargo fmt`.