# Releasing

A `v*` tag runs [`.github/workflows/release.yml`](../.github/workflows/release.yml):

| Job         | Runs in                 | Publishes                                                                  |
| ----------- | ----------------------- | -------------------------------------------------------------------------- |
| `verify`    | any repository          | Nothing; fails unless the tag equals the `Cargo.toml` and `sdk/node` versions |
| `binaries`, `release` | any repository | Linux/macOS archives, per-file `.sha256`, `SHA256SUMS`, a GitHub Release  |
| `image`     | any repository          | `ghcr.io/<owner>/<repository>` for linux/amd64 and linux/arm64             |
| `npm`       | `aevantec/idbseq` only  | npm package `idbseq`, with provenance                                      |
| `packagist` | `aevantec/idbseq` only  | Splits `sdk/php` into `aevantec/idbseq-php` and updates Packagist `idbseq/idbseq` |

Tags with a `-` (for example `v0.2.0-rc.1`) become GitHub prereleases, get no
`latest` or `major.minor` image tags, and go to npm under the `next` dist-tag.
Nothing is published to crates.io.

## One-time setup (public repository)

1. **npm** (package `idbseq`, already owned by the maintainer account):
   - Preferred: on npmjs.com, open the package's **Settings → Trusted publishing**
     and add GitHub Actions with repository `aevantec/idbseq`, workflow
     `release.yml`, environment `npm`. No token is stored in GitHub.
   - Otherwise: create a granular access token with publish rights on `idbseq`
     and save it as the `NPM_TOKEN` secret of the `npm` environment.
   - Turn on two-factor authentication for the account.
2. **PHP mirror**: create the public repository `aevantec/idbseq-php` (empty, no
   README) with the description "Read-only mirror of sdk/php in aevantec/idbseq".
   Create a fine-grained token with **Contents: read and write** on that
   repository only, and save it as the `PHP_MIRROR_TOKEN` secret of the
   `packagist` environment.
3. **Packagist**: after the first release has filled the mirror, submit
   `https://github.com/aevantec/idbseq-php` on packagist.org (this claims
   `idbseq/idbseq`). Save your Packagist username and API token as the
   `PACKAGIST_USERNAME` and `PACKAGIST_TOKEN` secrets of the `packagist`
   environment. The first `packagist` job fails at "Ask Packagist to update"
   until then; re-run it after submitting.
4. **GHCR**: after the first image push, open the `idbseq` package on GitHub
   (**Packages → idbseq → Package settings**), set visibility to **Public**, and
   link it to the `aevantec/idbseq` repository.
5. **Environments**: create `npm` and `packagist` under **Settings →
   Environments**; optionally require a reviewer so every publish needs an
   approval.

## Cutting a release

1. Update the version in `Cargo.toml` (`[workspace.package]`) and in
   `sdk/node/package.json` (and its `package-lock.json`, via
   `npm version <x.y.z> --no-git-tag-version` in `sdk/node`). The PHP package
   takes its version from the tag.
2. Make sure CI and SDKs are green on `main`, and `api/openapi.json` is current.
3. Tag and push:

   ```bash
   git tag -a v0.1.0 -m "v0.1.0"
   git push origin v0.1.0
   ```

4. Check the run, then the results: the GitHub Release, `docker pull
   ghcr.io/aevantec/idbseq:0.1.0`, `npm view idbseq version`, and the version on
   packagist.org.

`sdk/node/package.json` keeps `"private": true` so nobody can publish from a
laptop by accident; the `npm` job removes it just before publishing.
