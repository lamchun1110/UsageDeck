import fs from 'node:fs';
import path from 'node:path';

const root = new URL('../../', import.meta.url);

function readRustSources() {
  const directory = new URL('src-tauri/src/', root);
  const files = [];
  const walk = (folder) => {
    for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
      if (entry.name === 'tests') continue;
      const full = path.join(folder.pathname, entry.name);
      if (entry.isDirectory()) walk(new URL(`${entry.name}/`, folder));
      else if (entry.name.endsWith('.rs')) files.push(fs.readFileSync(full, 'utf8'));
    }
  };
  walk(directory);
  return files.join('\n');
}

const rustSource = readRustSources();
const frontendSource = fs.readFileSync(new URL('src/lib/backend.ts', root), 'utf8');

const emittedEvents = new Set(
  [...rustSource.matchAll(/\.emit\(\s*"([a-zA-Z0-9_-]+)"/g)].map((match) => match[1]),
);
// The frontend listens through onEvent (which wraps @tauri-apps listen) plus a
// few direct listen<T> call sites.
const listenedEvents = new Set([
  ...[...frontendSource.matchAll(/onEvent(?:<[^>]+>)?\(\s*'([a-zA-Z0-9_-]+)'/g)].map((m) => m[1]),
  ...[...frontendSource.matchAll(/\blisten(?:<[^>]+>)?\(\s*'([a-zA-Z0-9_-]+)'/g)].map((m) => m[1]),
]);

if (emittedEvents.size === 0) throw new Error('No Rust emit() sites found; the scan is broken.');

const neverListened = [...emittedEvents].filter((event) => !listenedEvents.has(event));
const neverEmitted = [...listenedEvents].filter((event) => !emittedEvents.has(event));
if (neverListened.length > 0 || neverEmitted.length > 0) {
  throw new Error(
    `Tauri event contract mismatch; emitted but never listened to: ${
      neverListened.join(', ') || 'none'
    }; listened to but never emitted: ${neverEmitted.join(', ') || 'none'}`,
  );
}

console.log(`${emittedEvents.size} Tauri event names match between Rust emit and frontend listen.`);
