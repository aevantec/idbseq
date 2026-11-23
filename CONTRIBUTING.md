# Contributing

Thanks for helping with idbseq. Security problems go through [SECURITY.md](SECURITY.md),
not public issues.

## Workflow

1. Open an issue first for anything bigger than a small fix, so the approach can be agreed.
2. Branch from `main` (`feat/…`, `fix/…`, `docs/…`).
3. Open a pull request against `main`. `main` is protected: the **CI** and **SDKs**
   workflows must pass before a maintainer merges it.
4. Commits follow `<type>: <description>` with types `feat`, `fix`, `refactor`, `docs`,
   `test`, `chore`, `perf`, `ci`, and no scope (`feat: …`, not `feat(api): …`). Pull
   requests are squash-merged, so the pull request title becomes the commit subject and
   follows the format too; write reverts as, for example,
   `docs: revert the IdbSeq rename in READMEs`. CI checks every new
   commit; to check locally before committing, run once per clone:

   ```bash
   git config core.hooksPath .githooks
   ```

## Before you push

Run the same gates CI runs:

```bash
cargo fmt --all --check
cargo lint # clippy, warnings are errors
cargo test --workspace --locked
cargo llvm-cov --workspace --locked --fail-under-lines 80 # needs cargo-llvm-cov
cargo deny check                                          # needs cargo-deny
```

If you touch an SDK:

```bash
(cd sdk/node && npm ci && npm run typecheck && npm test && npm run test:package)
scripts/e2e.sh # every client against a throwaway idbseqd
```

If you touch the documentation (`docs/`, or a README the site includes), preview
and build the site; the build fails on dead links:

```bash
(cd docs && npm ci && npm run dev)   # http://localhost:5173/idbseq/
(cd docs && npm run build)
```

If you touch the admin console (`crates/idbseq-server/ui/`: Vue 3, Tailwind CSS
and shadcn-vue), run it against a local `idbseqd` with hot reload, then rebuild
and **commit `ui/dist/`**. The server embeds `dist/` at compile time, so building
it needs no Node, and CI fails when `dist/` does not match the sources:

```bash
cd crates/idbseq-server/ui
npm ci
npm run dev            # http://localhost:5173/ui/, API proxied to 127.0.0.1:7878 (IDBSEQ_URL)
npm test               # unit tests with an 80% coverage gate
npm run build          # typecheck, then write dist/
cd ../../.. && scripts/e2e.sh ui   # Playwright, against a throwaway idbseqd
```

Add UI building blocks with `npx shadcn-vue@2.9.0 add <component>`, and avoid
any that render inline `<style>` elements (reka-ui's Select does): the console's
Content-Security-Policy blocks them, and the end-to-end tests fail on the
resulting console error.

The logo ("id" and "+", an incrementing ID) is `docs/public/logo.svg`, with
`logo-dark.svg` for dark backgrounds. The console's favicon is a copy and its
mark draws the same path from `ui/src/lib/brand.ts`; a unit test fails if they
drift apart, so change all three together.

If you change the HTTP API, regenerate the contract and commit it with the change:

```bash
cargo run -q -p idbseq-server --bin idbseqd -- openapi > api/openapi.json
```

## Expectations

- Tests come with the change: unit tests next to the code, integration tests under
  `tests/`. Coverage stays at or above 80% lines.
- The wire contract in [docs/API.md](docs/API.md) is the source of truth; keep it,
  the OpenAPI file and the Node.js SDK in step.
- Decisions that change guarantees (uniqueness, durability, auth) get an ADR in
  [docs/adr/](docs/adr/README.md).
- Performance-sensitive changes: run the benchmarks in [docs/benchmarks.md](docs/benchmarks.md)
  and compare with `benches/baseline.md`.

By contributing you agree that your contributions are licensed under the
[MIT License](LICENSE).
