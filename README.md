# desparrow

A discrete-event spacecraft simulation engine with the hubris to be named
after the fastest bird.

- [Install rust](https://www.rust-lang.org/tools/install)
- Build with `cargo build`
- Test with `cargo test`
- Build docs with `cargo doc` (then open `target/doc/desparrow/index.html`).

Desparrow is designed to be used with branching iterative schedulers, through incremental
simulation, long-term simulation state history, targeted resource queries, and parallelism.
The documentation is still in progress but should contain enough examples for you to see
what the project is about.

## Todo

- [ ] Caches should first check whether their inputs have actually changed before dropping the cache.
- [ ] Sim and maybe plan objects should provide a timeline of revisions so that sims can fork
