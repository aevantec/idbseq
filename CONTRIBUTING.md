# Contributing

Thanks for helping with idbseq. Security problems go through [SECURITY.md](SECURITY.md),
not public issues.

## Workflow


1. Open an issue first for anything bigger than a small fix, so the approach can be agreed.
2. Branch from `main` (`feat/…`, `fix/…`, `docs/…`).
3. Open a pull request against `main`. `main` is protected: the **CI** and **SDKs**
   workflows must pass and a maintainer must approve before it merges.
4. Commits follow `<type>: <description>` with types `feat`, `fix`, `refactor`, `docs`,
   `test`, `chore`, `perf`, `ci`, and no scope (`feat: …`, not `feat(api): …`). Merges
   and reverts follow it too: prefer fast-forward or squash merges, and write reverts as,
   for example, `docs: revert the IdbSeq rename in READMEs`. CI checks every new
   commit; to check locally before committing, run once per clone:

   ```bash
   git config core.hooksPath .githooks
   ```

## Before you push

Run the same gates CI runs:

```bash
cargo fmt --all --check
cargo lint                                                   # clippy, warnings are errors
cargo test --workspace --locked
cargo llvm-cov --workspace --locked --fail-under-lines 80    # needs cargo-llvm-cov
cargo deny check                                             # needs cargo-deny
```

If you touch an SDK:

```bash
(cd sdk/node && npm ci && npm run typecheck && npm test && npm run test:package)
(cd sdk/php && composer update && composer phpstan && composer test:coverage)
scripts/e2e.sh                     # every client against a throwaway idbseqd
```

If you change the HTTP API, regenerate the contract and commit it with the change:

```bash
cargo run -q -p idbseq-server --bin idbseqd -- openapi > api/openapi.json
```

## Expectations

- Tests come with the change: unit tests next to the code, integration tests under
  `tests/`. Coverage stays at or above 80% lines.
- The wire contract in [docs/API.md](docs/API.md) is the source of truth; keep it,
  the OpenAPI file and both SDKs in step.
- Decisions that change guarantees (uniqueness, durability, auth) get an ADR in
  [docs/adr/](docs/adr/README.md).
- Performance-sensitive changes: run the benchmarks in [docs/benchmarks.md](docs/benchmarks.md)
  and compare with `benches/baseline.md`.

By contributing you agree that your contributions are licensed under the
[MIT License](LICENSE).
