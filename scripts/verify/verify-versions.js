import fs from 'node:fs';

const root = new URL('../../', import.meta.url);
const packageJson = JSON.parse(fs.readFileSync(new URL('package.json', root), 'utf8'));
const tauriConfig = JSON.parse(fs.readFileSync(new URL('src-tauri/tauri.conf.json', root), 'utf8'));
const cargoManifest = fs.readFileSync(new URL('src-tauri/Cargo.toml', root), 'utf8');
const cargoPackage = cargoManifest.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);

if (!cargoPackage) {
  throw new Error('Could not read the package version from src-tauri/Cargo.toml.');
}

const versions = {
  package: packageJson.version,
  tauri: tauriConfig.version,
  cargo: cargoPackage[1],
};
const uniqueVersions = new Set(Object.values(versions));

if (uniqueVersions.size !== 1) {
  throw new Error(
    `UsageDeck version mismatch: ${Object.entries(versions)
      .map(([source, version]) => `${source}=${version}`)
      .join(', ')}`,
  );
}

const expectedTag = process.argv[2];
if (expectedTag && expectedTag !== `v${versions.package}`) {
  throw new Error(
    `Release tag ${expectedTag} does not match application version ${versions.package}.`,
  );
}

console.log(`UsageDeck version ${versions.package} is consistent.`);
