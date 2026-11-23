// Prepares the npm packages of a release (run by .github/workflows/release.yml,
// and locally to try a release without publishing):
//
//   node scripts/npm-release.mjs platforms <native-dir> <out-dir> <version>
//     One package per embedded module found in <native-dir> (files named
//     idbseq.<target>.node): <out-dir>/<target>/ holds @idbseq/<target>.
//     With --all, every target in TARGETS must be there.
//
//   node scripts/npm-release.mjs main <package.json> <version> [--mcp]
//     Readies a package.json for publishing: drops `private` (a guard against
//     publishing from a laptop) and pins the version. For `idbseq`, lists every
//     platform package as an optional dependency, so npm installs the one that
//     matches the machine; with --mcp (idbseq-mcp), depends on that `idbseq`.
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

/** napi-rs target names, as `nativeTarget()` in src/native.cts computes them. */
export const TARGETS = {
  'darwin-arm64': { os: 'darwin', cpu: 'arm64' },
  'darwin-x64': { os: 'darwin', cpu: 'x64' },
  'linux-arm64-gnu': { os: 'linux', cpu: 'arm64', libc: 'glibc' },
  'linux-arm64-musl': { os: 'linux', cpu: 'arm64', libc: 'musl' },
  'linux-x64-gnu': { os: 'linux', cpu: 'x64', libc: 'glibc' },
  'linux-x64-musl': { os: 'linux', cpu: 'x64', libc: 'musl' },
  'win32-arm64-msvc': { os: 'win32', cpu: 'arm64' },
  'win32-x64-msvc': { os: 'win32', cpu: 'x64' },
};

const SCOPE = '@idbseq';
const VERSION = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;
const mainManifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));

function fail(message) {
  process.stderr.write(`npm-release: ${message}\n`);
  process.exit(1);
}

function checkVersion(version) {
  if (!VERSION.test(version ?? '')) fail(`not a version: ${version}`);
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function platforms(nativeDir, outDir, version, isAll) {
  checkVersion(version);
  const found = Object.keys(TARGETS).filter((target) => existsSync(join(nativeDir, `idbseq.${target}.node`)));
  const missing = Object.keys(TARGETS).filter((target) => !found.includes(target));
  if (found.length === 0) fail(`no idbseq.<target>.node files in ${nativeDir}`);
  if (isAll && missing.length > 0) fail(`missing modules: ${missing.join(', ')}`);
  for (const target of found) {
    const { os, cpu, libc } = TARGETS[target];
    const file = `idbseq.${target}.node`;
    const dir = join(outDir, target);
    mkdirSync(dir, { recursive: true });
    copyFileSync(join(nativeDir, file), join(dir, file));
    writeJson(join(dir, 'package.json'), {
      name: `${SCOPE}/${target}`,
      version,
      description: `The embedded idbseq database for ${target}. Installed by the idbseq package; do not depend on it directly.`,
      license: mainManifest.license,
      repository: mainManifest.repository,
      homepage: mainManifest.homepage,
      main: file,
      files: [file],
      os: [os],
      cpu: [cpu],
      ...(libc ? { libc: [libc] } : {}),
      engines: mainManifest.engines,
    });
    writeFileSync(
      join(dir, 'README.md'),
      `# ${SCOPE}/${target}\n\nThe embedded idbseq database module for ${target}. ` +
        'npm installs it with the [`idbseq`](https://www.npmjs.com/package/idbseq) package; use that one.\n',
    );
    process.stdout.write(`${join(dir, file)}\n`);
  }
}

function main(manifestPath, version, isMcp) {
  checkVersion(version);
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  delete manifest.private;
  manifest.version = version;
  if (isMcp) {
    manifest.dependencies = { ...manifest.dependencies, idbseq: version };
  } else {
    manifest.optionalDependencies = Object.fromEntries(
      Object.keys(TARGETS).map((target) => [`${SCOPE}/${target}`, version]),
    );
  }
  writeJson(manifestPath, manifest);
  process.stdout.write(`${manifest.name}@${version}\n`);
}

const [command, ...args] = process.argv.slice(2);
const flags = new Set(args.filter((arg) => arg.startsWith('--')));
const [first, second, third] = args.filter((arg) => !arg.startsWith('--'));
if (command === 'platforms' && third) platforms(first, second, third, flags.has('--all'));
else if (command === 'main' && second) main(first, second, flags.has('--mcp'));
else fail('usage: platforms <native-dir> <out-dir> <version> [--all] | main <package.json> <version> [--mcp]');
