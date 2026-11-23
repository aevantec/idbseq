// The API explorer page reads openapi.json from the site itself; the committed
// contract in api/openapi.json stays the only source.
import { copyFileSync, mkdirSync } from 'node:fs';

const source = new URL('../../../api/openapi.json', import.meta.url);
const target = new URL('../../public/openapi.json', import.meta.url);

mkdirSync(new URL('.', target), { recursive: true });
copyFileSync(source, target);
