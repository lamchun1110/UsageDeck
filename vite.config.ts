import { configDefaults, defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { readFileSync } from 'node:fs';

const packageVersion = (
  JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as {
    version: string;
  }
).version;

export default defineConfig({
  define: {
    'import.meta.env.APP_VERSION': JSON.stringify(packageVersion),
  },
  plugins: [svelte()],
  resolve: {
    conditions: ['browser'],
  },
  clearScreen: false,
  server: {
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: true,
    // The App-level suites render the whole panel under jsdom; coverage
    // instrumentation multiplies their runtime past the 5s default.
    testTimeout: 20_000,
    exclude: [...configDefaults.exclude, '.local/**'],
    coverage: {
      provider: 'v8',
      include: ['src/**'],
      exclude: ['src/test/**', 'src/lib/messages/**', 'src/main.ts', 'src/vite-env.d.ts'],
      reporter: ['text', 'html', 'lcov'],
      // Floors set just under the suite's current levels so the report can
      // only get greener; raise them as component coverage fills in.
      thresholds: {
        statements: 80,
        branches: 68,
        functions: 82,
        lines: 82,
      },
    },
  },
});
