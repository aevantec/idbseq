// Checks that the ESM build loads through the package exports map.
import assert from 'node:assert/strict';
import { IdbSeqClient, ProtectedCollectionError, parseJson } from 'idbseq';

assert.equal(typeof IdbSeqClient, 'function');
assert.equal(typeof ProtectedCollectionError, 'function');
assert.equal(parseJson('{"id":9223372036854775807}').id, 9223372036854775807n);
assert.equal(parseJson('{"id":42}').id, 42);
process.stdout.write('esm build ok\n');
