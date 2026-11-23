// Loads the native module behind IdbSeqDatabase. This file compiles to
// CommonJS in both builds, so `require` and `__dirname` are available.
import { existsSync } from 'node:fs';
import { join } from 'node:path';

/** The class exported by the Rust module (crates/idbseq-node). */
export interface NativeDatabase {
  /** Resolves to a JSON envelope: `{ data, meta? }` or `{ error }`. Never rejects. */
  call(op: string, args: string): Promise<string>;
}

export interface NativeModule {
  readonly NativeDatabase: new (path: string, shared: boolean, busyTimeoutMs: number) => NativeDatabase;
}

/** The napi-rs target name for this machine, e.g. `darwin-arm64` or `linux-x64-gnu`. */
export function nativeTarget(platform: string = process.platform, arch: string = process.arch): string {
  if (platform === 'linux') {
    return `linux-${arch}-${isMusl() ? 'musl' : 'gnu'}`;
  }
  if (platform === 'win32') {
    return `win32-${arch}-msvc`;
  }
  return `${platform}-${arch}`;
}

function isMusl(): boolean {
  const report = process.report?.getReport() as { header?: { glibcVersionRuntime?: string } } | undefined;
  return report?.header?.glibcVersionRuntime === undefined;
}

let loaded: NativeModule | undefined;

/**
 * The native module: `native/idbseq.<target>.node` inside this package (a local
 * build), else the `@idbseq/<target>` package, which npm installs as an
 * optional dependency of `idbseq` on matching machines.
 */
export function loadNative(): NativeModule {
  loaded ??= load(nativeTarget());
  return loaded;
}

function load(target: string): NativeModule {
  const file = `idbseq.${target}.node`;
  // From src/ (tests) and from dist/esm or dist/cjs (the built package).
  const local = [join(__dirname, '..', 'native', file), join(__dirname, '..', '..', 'native', file)];
  const candidates = [...local.filter((path) => existsSync(path)), `@idbseq/${target}`];
  const failures: string[] = [];
  for (const candidate of candidates) {
    try {
      return require(candidate) as NativeModule;
    } catch (error) {
      failures.push(`${candidate}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  throw new Error(
    `idbseq: the embedded database module for ${target} is not installed. npm installs ` +
      `@idbseq/${target} with idbseq unless optional dependencies are turned off ` +
      '(--omit=optional, --no-optional) or the lockfile was made on another platform: ' +
      'reinstall idbseq on this machine. On other platforms, use IdbSeqClient with an idbseq server.\n' +
      failures.join('\n'),
  );
}
