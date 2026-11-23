# Releasing

A `v*` tag runs [`.github/workflows/release.yml`](../.github/workflows/release.yml):

| Job                 | Runs in                | Does                                                                                   |
| ------------------- | ---------------------- | -------------------------------------------------------------------------------------- |
| `verify`            | any repository         | Fails unless `Cargo.toml`, `sdk/node` and `sdk/mcp` share the tag's version and the tagged commit is on `main` |
| `binaries`          | any repository         | `idbseqd` and `idbseq` for Linux (gnu, musl), macOS and Windows, x86_64 and arm64      |
| `release`           | any repository         | A GitHub Release with the archives, per-file `.sha256` and `SHA256SUMS`                |
| `image`             | any repository         | `ghcr.io/<owner>/<repository>` for linux/amd64 and linux/arm64                         |
| `native`            | any repository         | The embedded module for 8 platforms                                                    |
| `npm-pack`          | any repository         | Tests the Node SDK and the MCP server, packs `@idbseq/<platform>`, `idbseq`, `idbseq-mcp` |
| `npm-smoke`         | any repository         | Installs the packed packages on Linux (glibc, musl), macOS and Windows and uses them   |
| `npm-publish`       | `aevantec/idbseq` only | Publishes the packed packages, with provenance: platforms first, then `idbseq`, then `idbseq-mcp` |
| `crates`            | `aevantec/idbseq` only | Publishes `idbseq-core`, then `idbseq`, to crates.io                                   |

Nothing is published before every build and test has passed and the GitHub
Release exists. The publish jobs skip what is already out, so a failed release
can be re-run. Tags with a `-` (for example `v0.2.0-rc.1`) become GitHub
prereleases, get no `latest` or `major.minor` image tags, and go to npm under
the `next` dist-tag.

**Dry run:** **Actions → Release → Run workflow** on `main` builds and tests
everything (8 binaries, 8 modules, the image, the npm installs) and publishes
nothing. Do one before the first release, and after changing the workflow.

The server and CLI crates (`idbseq-server`, `idbseq-cli`) and the Node binding
(`idbseq-node`) are not on crates.io: they ship as binaries and npm packages.

## Registry setup

One time, in `aevantec/idbseq`. Repeat it only for a fork, or after moving a
package to another account.

1. **npm**:
   - Create the free organization **`idbseq`** on npmjs.com (it owns the
     `@idbseq/<platform>` packages). Do it early: whoever creates it first owns
     the scope.
   - The account already owns `idbseq`; reserve `idbseq-mcp` with a placeholder
     the same way.
   - Create a granular access token with read and write on the `@idbseq`
     organization's packages and on `idbseq` and `idbseq-mcp`, and save it as
     the `NPM_TOKEN` secret of the `npm` environment. A token is needed for the
     first release: npm trusted publishing can only be set up on packages that
     already exist. Afterwards, adding a trusted publisher (repository
     `aevantec/idbseq`, workflow `release.yml`, environment `npm`) to each
     package lets you delete the token.
   - Turn on two-factor authentication for the account.
2. **crates.io**: sign in with GitHub, create an API token with the
   `publish-new` and `publish-update` scopes (limit it to `idbseq` and
   `idbseq-core` after the first release), and save it as the
   `CARGO_REGISTRY_TOKEN` secret of the `crates-io` environment.
3. **Environments** (**Settings → Environments**): create `npm` and
   `crates-io`. For each: **Required reviewers** (every publish waits for an
   approval) and **Deployment branches and tags → Selected**, with the tag
   pattern `v*`.
4. **Tag ruleset** (**Settings → Rules → Rulesets → New tag ruleset**): target
   `v*`, restrict creations, updates and deletions to the maintainers, and
   block force pushes. The workflow also refuses a tag whose commit is not on
   `main`.
5. **GHCR**: once an image has been pushed, open the `idbseq` package on GitHub
   (**Packages → idbseq → Package settings**), set visibility to **Public**, and
   link it to the `aevantec/idbseq` repository.

## Cutting a release

1. Set the new version in:
   - `Cargo.toml`: `version` under `[workspace.package]`, and the `=x.y.z` of
     `idbseq-core` under `[workspace.dependencies]` (crates.io gets both crates
     at the same version). Then `cargo check` to update `Cargo.lock`.
   - `sdk/node` and `sdk/mcp`: `npm version <x.y.z> --no-git-tag-version` in
     each.
2. Make sure CI and SDKs are green on `main`, and `api/openapi.json` is current.
3. Tag the release commit on `main` of `aevantec/idbseq` and push the tag:

   ```bash
   git tag -a v0.2.0 -m "v0.2.0"
   git push origin v0.2.0
   ```

4. Approve the `npm` and `crates-io` deployments when the run asks, then check
   the results: the GitHub Release, `docker pull ghcr.io/aevantec/idbseq:0.2.0`,
   `npm view idbseq version`, `npm view idbseq-mcp version` and
   [crates.io/crates/idbseq](https://crates.io/crates/idbseq).

## Documentation website

[`.github/workflows/docs.yml`](../.github/workflows/docs.yml) builds the VitePress
site in `docs/` and checks links on every change. In `aevantec/idbseq` it also
deploys to GitHub Pages on each `v*` tag, and on demand from `main`
(**Actions → Docs → Run workflow**), at `https://aevantec.github.io/idbseq/`.

One-time setup: **Settings → Pages → Build and deployment → Source: GitHub
Actions**. The workflow creates the `github-pages` environment on its first run.

Custom domain (optional, e.g. `idbseq.dev`):

1. Add the domain under **Settings → Pages → Custom domain** and create the DNS
   record GitHub shows (a `CNAME` to `aevantec.github.io` for a subdomain, or the
   `A`/`AAAA` records for an apex domain). Turn on **Enforce HTTPS**.
2. Commit `docs/public/CNAME` containing the domain, so each deploy keeps it.
3. Serve the site from the root: set `DOCS_BASE: /` on the build step in
   `docs.yml` (the default base is `/idbseq/`).

## Private flag

`sdk/node/package.json` and `sdk/mcp/package.json` keep `"private": true` so nobody
can publish from a laptop by accident. `sdk/node/scripts/npm-release.mjs` prepares
the copies that the release packs: it drops the flag, sets the version, lists the
platform packages as optional dependencies of `idbseq`, and points `idbseq-mcp` at
the released `idbseq` instead of `file:../node`.
