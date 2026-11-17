import { defineConfig } from 'vite';
import { validateEngineArtifact } from './scripts/check-engine-artifact.mjs';

export default defineConfig({
  // Keep assets and the WASM URL portable across GitHub Pages repository names.
  base: './',
  plugins: [{
    name: 'agar-engine-contract',
    // Validate the actual bundled binary, including when Vite is run directly.
    async buildStart() { await validateEngineArtifact(); },
  }],
  build: { target: 'es2022' },
  server: { host: '0.0.0.0', allowedHosts: true },
});
