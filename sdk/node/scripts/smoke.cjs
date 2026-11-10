// Checks that the CommonJS build loads through the package exports map.
const assert = require('node:assert/strict');
const sdk = require('idbseq');

assert.equal(typeof sdk.IdbseqClient, 'function');
assert.equal(typeof sdk.ProtectedCollectionError, 'function');
assert.equal(sdk.parseJson('{"id":9223372036854775807}').id, 9223372036854775807n);
assert.equal(sdk.parseJson('{"id":42}').id, 42);
process.stdout.write('cjs build ok\n');
