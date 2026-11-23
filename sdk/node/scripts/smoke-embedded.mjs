// Checks the embedded database through the package exports map: several
// processes take IDs from one file in shared mode and never get the same one.
// Needs the native module (npm run build:native).
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { IdbSeqDatabase } from 'idbseq';

const WORKERS = 3;
const IDS_PER_WORKER = 20;

if (process.argv[2] === 'worker') {
  const ids = await IdbSeqDatabase.open(process.argv[3], { shared: true, busyTimeoutMs: 60_000 });
  const issued = [];
  for (let i = 0; i < IDS_PER_WORKER; i += 1) {
    issued.push(await ids.generateId('cards'));
  }
  await ids.close();
  process.stdout.write(issued.join('\n'));
} else {
  const dir = mkdtempSync(join(tmpdir(), 'idbseq-smoke-'));
  try {
    const path = join(dir, 'shared.idbseq');
    const setup = await IdbSeqDatabase.open(path, { shared: true });
    await setup.createCollection({ name: 'cards' });
    await setup.close();

    const run = promisify(execFile);
    const script = fileURLToPath(import.meta.url);
    const outputs = await Promise.all(
      Array.from({ length: WORKERS }, () => run(process.execPath, [script, 'worker', path])),
    );
    const ids = outputs.flatMap(({ stdout }) => stdout.split('\n').map(Number));

    assert.equal(ids.length, WORKERS * IDS_PER_WORKER);
    assert.equal(new Set(ids).size, ids.length, 'an ID was issued twice');
    assert.equal(Math.max(...ids), ids.length, 'shared mode leaves no gaps');
    process.stdout.write(`embedded ok (${WORKERS} processes, ${ids.length} IDs)\n`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
