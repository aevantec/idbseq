// Installs the packed npm release (idbseq + the @idbseq/<target> package for
// this machine) into a fresh project and uses the embedded database through
// it, as a user would. Run by the release workflow on each OS before anything
// is published.
//
// Usage: node scripts/smoke-installed.mjs <dir with the .tgz files>
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

/** The napi-rs target name, as src/native.cts in the Node SDK computes it. */
function nativeTarget() {
  const { platform, arch } = process;
  if (platform === 'linux') {
    const glibc = process.report?.getReport()?.header?.glibcVersionRuntime;
    return `linux-${arch}-${glibc === undefined ? 'musl' : 'gnu'}`;
  }
  return platform === 'win32' ? `win32-${arch}-msvc` : `${platform}-${arch}`;
}

const packages = resolve(process.argv[2] ?? '');
const files = readdirSync(packages);
const target = nativeTarget();
const pick = (pattern) => {
  const file = files.find((name) => pattern.test(name));
  if (!file) throw new Error(`no package matching ${pattern} in ${packages}`);
  return join(packages, file);
};
const main = pick(/^idbseq-\d.*\.tgz$/);
const platform = pick(new RegExp(`^idbseq-${target}-\\d.*\\.tgz$`));

const app = mkdtempSync(join(tmpdir(), 'idbseq-smoke-'));
writeFileSync(join(app, 'package.json'), '{ "name": "smoke", "private": true }\n');
// npm cannot fetch the other platforms' packages before they are published;
// it skips them because they are optional.
execFileSync('npm', ['install', '--no-audit', '--no-fund', main, platform], {
  cwd: app,
  stdio: 'inherit',
  shell: process.platform === 'win32',
});

const check = `
const { IdbSeqDatabase } = require('idbseq');
(async () => {
  const db = await IdbSeqDatabase.open('smoke.idbseq');
  await db.createCollection({ name: 'smoke', start: 41 });
  const first = await db.generateId('smoke');
  await db.close();
  const shared = await IdbSeqDatabase.open('smoke.idbseq', { shared: true });
  const second = await shared.generateId('smoke');
  await shared.close();
  if (first !== 41 || second !== 42) throw new Error('got ' + first + ', ' + second);
  console.log('${target}: installed package issued 41, 42');
})().catch((error) => { console.error(error); process.exit(1); });
`;
execFileSync(process.execPath, ['-e', check], { cwd: app, stdio: 'inherit' });
