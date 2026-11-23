// Builds the embedded database module from the Rust workspace and copies it to
// native/idbseq.<target>.node, where the package looks for it first.
// Usage: npm run build:native [-- --debug]
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const debug = process.argv.includes('--debug');
const workspace = new URL('../../../', import.meta.url);
const profile = debug ? 'debug' : 'release';

execFileSync('cargo', ['build', '-p', 'idbseq-node', '--locked', ...(debug ? [] : ['--release'])], {
  cwd: workspace,
  stdio: 'inherit',
});

const library =
  {
    darwin: 'libidbseq_node.dylib',
    win32: 'idbseq_node.dll',
  }[process.platform] ?? 'libidbseq_node.so';

// Where cargo builds: CARGO_TARGET_DIR or build.target-dir may move it from target/.
const { target_directory: targetDir } = JSON.parse(
  execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], { cwd: workspace, encoding: 'utf8' }),
);

// The target name comes from the loader itself, so the two always agree.
const { nativeTarget } = require('../dist/cjs/native.cjs');
const target = nativeTarget();
mkdirSync(new URL('../native/', import.meta.url), { recursive: true });
copyFileSync(
  join(targetDir, profile, library),
  fileURLToPath(new URL(`../native/idbseq.${target}.node`, import.meta.url)),
);
process.stdout.write(`native/idbseq.${target}.node (${profile})\n`);
