import { readFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

export const DEFAULT_ENGINE_ARTIFACT = fileURLToPath(new URL('../engine/pkg/agar_engine.wasm', import.meta.url));
const CONTRACT_FILE = new URL('../src/engine-contract.json', import.meta.url);
const MAX_ARTIFACT_BYTES = 16 * 1024 * 1024;
const REBUILD = 'Run npm run build:wasm, then rebuild the frontend with npm run build or npm run build:pages.';

/** Validate the actual binary that Vite will bundle; no Rust toolchain is needed. */
export async function validateEngineArtifact(file = DEFAULT_ENGINE_ARTIFACT) {
  const filename = file instanceof URL ? fileURLToPath(file) : resolve(file);
  const fail = reason => new Error(`Invalid agar engine artifact (${filename}): ${reason} ${REBUILD}`);
  let contract;
  try {
    contract = JSON.parse(await readFile(CONTRACT_FILE, 'utf8'));
  } catch (error) {
    throw fail(`The engine export contract could not be read: ${error.message}.`);
  }
  if (!contract || !Number.isInteger(contract.abi) || contract.abi < 1
    || typeof contract.memory !== 'string' || !contract.memory
    || !Array.isArray(contract.functions) || !contract.functions.length
    || !contract.functions.every(name => typeof name === 'string' && name)
    || new Set(contract.functions).size !== contract.functions.length
    || !contract.functions.includes('abi_version') || contract.functions.includes(contract.memory)) {
    throw fail('The engine export contract is malformed.');
  }
  let bytes;
  try {
    const information = await stat(filename);
    if (!information.isFile() || information.size < 8 || information.size > MAX_ARTIFACT_BYTES) {
      throw new Error('Expected a WASM file between 8 bytes and 16 MiB');
    }
    bytes = await readFile(filename);
    if (bytes.length < 8 || bytes.length > MAX_ARTIFACT_BYTES) throw new Error('The WASM file changed size while being read');
  } catch (error) {
    throw fail(`The binary could not be read: ${error.message}.`);
  }
  let module;
  try {
    module = await WebAssembly.compile(bytes);
  } catch (error) {
    throw fail(`The file is not a valid WebAssembly module: ${error.message}.`);
  }
  const exports = WebAssembly.Module.exports(module);
  const byName = new Map(exports.map(item => [item.name, item.kind]));
  const missing = contract.functions.filter(name => !byName.has(name));
  const wrongKinds = contract.functions.filter(name => byName.has(name) && byName.get(name) !== 'function')
    .map(name => `${name} is ${byName.get(name)}, expected function`);
  if (!byName.has(contract.memory)) missing.push(contract.memory);
  else if (byName.get(contract.memory) !== 'memory') wrongKinds.push(`${contract.memory} is ${byName.get(contract.memory)}, expected memory`);
  if (missing.length || wrongKinds.length) {
    throw fail([
      missing.length ? `Missing engine exports: ${missing.join(', ')}.` : '',
      wrongKinds.length ? `Incorrect export types: ${wrongKinds.join('; ')}.` : '',
    ].filter(Boolean).join(' '));
  }
  let instance, abi;
  try {
    instance = await WebAssembly.instantiate(module, {});
    abi = instance.exports.abi_version();
  } catch (error) {
    throw fail(`The module could not initialize or report its ABI: ${error.message}.`);
  }
  if (!(instance.exports[contract.memory] instanceof WebAssembly.Memory)) throw fail(`The ${contract.memory} export is not WebAssembly memory.`);
  if (abi !== contract.abi) throw fail(`Expected engine ABI ${contract.abi}, received ${String(abi)}.`);
  return {
    file: filename, bytes, byteLength: bytes.length, module, instance, abi, exports,
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const result = await validateEngineArtifact(process.argv[2]);
    console.log(`Validated ${relative(process.cwd(), result.file)}: ABI ${result.abi}, ${result.exports.filter(item => item.kind === 'function').length} functions, ${result.byteLength.toLocaleString()} bytes.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
