import { mkdir, writeFile, rename, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { validateEngineArtifact } from './check-engine-artifact.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const installedCargo = join(homedir(), '.cargo', 'bin', process.platform === 'win32' ? 'cargo.exe' : 'cargo');
const cargo = process.env.CARGO || (existsSync(installedCargo) ? installedCargo : 'cargo');
const result = spawnSync(cargo, [
  'build', '--release', '--target', 'wasm32-unknown-unknown',
  '--manifest-path', join(root, 'engine', 'Cargo.toml'),
], { cwd: root, stdio: 'inherit' });

if (result.error) {
  console.error('Rust is required to rebuild the simulation. Install Rust from https://rustup.rs, then run: rustup target add wasm32-unknown-unknown');
  console.error(result.error.message);
  process.exit(1);
}
if (result.status !== 0) {
  console.error('WASM build failed. Check that the target is installed: rustup target add wasm32-unknown-unknown');
  process.exit(result.status ?? 1);
}

const targetDirectory = process.env.CARGO_TARGET_DIR
  ? resolve(root, process.env.CARGO_TARGET_DIR)
  : join(root, 'engine', 'target');
const binary = join(targetDirectory, 'wasm32-unknown-unknown', 'release', 'cellarium_engine.wasm');
// Validate Cargo's exact output before replacing the artifact imported by Vite.
const validated = await validateEngineArtifact(binary);
const destination = join(root, 'engine', 'pkg', 'agar_engine.wasm');
await mkdir(dirname(destination), { recursive: true });
const temporary = `${destination}.${process.pid}.tmp`;
try {
  await writeFile(temporary, validated.bytes);
  await rename(temporary, destination);
} finally {
  await rm(temporary, { force: true });
}
console.log(`Built and validated Rust simulation: engine/pkg/agar_engine.wasm (ABI ${validated.abi}, ${validated.byteLength.toLocaleString()} bytes)`);
