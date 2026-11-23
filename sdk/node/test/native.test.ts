import { describe, expect, it } from 'vitest';
import { loadNative, nativeTarget } from '../src/native.cjs';

describe('nativeTarget', () => {
  it('names targets the napi-rs way', () => {
    expect(nativeTarget('darwin', 'arm64')).toBe('darwin-arm64');
    expect(nativeTarget('win32', 'x64')).toBe('win32-x64-msvc');
    expect(nativeTarget('linux', 'arm64')).toMatch(/^linux-arm64-(gnu|musl)$/);
  });

  it('defaults to this machine', () => {
    expect(nativeTarget()).toContain(process.arch);
  });
});

describe('loadNative', () => {
  it('loads the module once and reuses it', () => {
    expect(loadNative()).toBe(loadNative());
    expect(typeof loadNative().NativeDatabase).toBe('function');
  });
});
