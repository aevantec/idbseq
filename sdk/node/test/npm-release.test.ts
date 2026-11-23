import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { nativeTarget } from '../src/native.cjs';

const SCRIPT = join(__dirname, '..', 'scripts', 'npm-release.mjs');
const ALL_TARGETS = [
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64-gnu',
  'linux-arm64-musl',
  'linux-x64-gnu',
  'linux-x64-musl',
  'win32-arm64-msvc',
  'win32-x64-msvc',
];

function run(...args: string[]) {
  const result = spawnSync(process.execPath, [SCRIPT, ...args], { encoding: 'utf8' });
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

function readJson(path: string) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

describe('npm-release.mjs', () => {
  let dir: string;
  let native: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'idbseq-npm-release-'));
    native = join(dir, 'native');
    mkdirSync(native);
  });

  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  const addModules = (targets: string[]) => {
    for (const target of targets) writeFileSync(join(native, `idbseq.${target}.node`), target);
  };

  it('makes one package per module, limited to its platform', () => {
    // Arrange
    addModules(ALL_TARGETS);

    // Act
    const result = run('platforms', native, join(dir, 'out'), '1.2.3', '--all');

    // Assert
    expect(result.status).toBe(0);
    expect(readdirSync(join(dir, 'out')).sort()).toEqual(ALL_TARGETS);
    const musl = readJson(join(dir, 'out', 'linux-x64-musl', 'package.json'));
    expect(musl).toMatchObject({
      name: '@idbseq/linux-x64-musl',
      version: '1.2.3',
      main: 'idbseq.linux-x64-musl.node',
      files: ['idbseq.linux-x64-musl.node'],
      os: ['linux'],
      cpu: ['x64'],
      libc: ['musl'],
      license: 'MIT',
    });
    expect(readJson(join(dir, 'out', 'win32-arm64-msvc', 'package.json')).libc).toBeUndefined();
    expect(readFileSync(join(dir, 'out', 'darwin-arm64', 'idbseq.darwin-arm64.node'), 'utf8')).toBe('darwin-arm64');
  });

  it('names each package as the loader looks for it', () => {
    // Arrange
    addModules(ALL_TARGETS);
    run('platforms', native, join(dir, 'out'), '1.2.3');

    for (const target of ALL_TARGETS) {
      // Act
      const { os, cpu, libc } = readJson(join(dir, 'out', target, 'package.json'));
      const expected = libc ? `linux-${cpu[0]}-${libc[0] === 'glibc' ? 'gnu' : 'musl'}` : nativeTarget(os[0], cpu[0]);

      // Assert
      expect(target).toBe(expected);
    }
  });

  it('refuses a release that misses a platform', () => {
    // Arrange
    addModules(ALL_TARGETS.filter((target) => target !== 'win32-arm64-msvc'));

    // Act
    const result = run('platforms', native, join(dir, 'out'), '1.2.3', '--all');

    // Assert
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('missing modules: win32-arm64-msvc');
  });

  it('refuses an empty folder and a bad version', () => {
    expect(run('platforms', native, join(dir, 'out'), '1.2.3').stderr).toContain('no idbseq.<target>.node files');
    addModules(['darwin-arm64']);
    expect(run('platforms', native, join(dir, 'out'), 'v1.2').stderr).toContain('not a version: v1.2');
    expect(run('nonsense').stderr).toContain('usage:');
  });

  it('readies idbseq: public, versioned, with every platform as optional', () => {
    // Arrange
    const manifest = join(dir, 'package.json');
    writeFileSync(manifest, JSON.stringify({ name: 'idbseq', version: '0.0.0', private: true }));

    // Act
    const result = run('main', manifest, '1.2.3-rc.1');

    // Assert
    expect(result.status).toBe(0);
    const ready = readJson(manifest);
    expect(ready.private).toBeUndefined();
    expect(ready.version).toBe('1.2.3-rc.1');
    expect(ready.optionalDependencies).toEqual(
      Object.fromEntries(ALL_TARGETS.map((target) => [`@idbseq/${target}`, '1.2.3-rc.1'])),
    );
  });

  it('points idbseq-mcp at the released idbseq', () => {
    // Arrange
    const manifest = join(dir, 'package.json');
    writeFileSync(
      manifest,
      JSON.stringify({ name: 'idbseq-mcp', private: true, dependencies: { idbseq: 'file:../node', zod: '^3' } }),
    );

    // Act
    run('main', manifest, '1.2.3', '--mcp');

    // Assert
    const ready = readJson(manifest);
    expect(ready.dependencies).toEqual({ idbseq: '1.2.3', zod: '^3' });
    expect(ready.optionalDependencies).toBeUndefined();
  });
});
