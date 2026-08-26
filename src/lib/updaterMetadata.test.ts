import { describe, expect, it } from 'vitest';
import { createUpdaterMetadata } from '../../.github/scripts/create-updater-json.mjs';

describe('release updater metadata', () => {
  const artifactNames = [
    'UsageDeck_0.2.0_x64-setup.exe',
    'UsageDeck_0.2.0_arm64-setup.exe',
    'UsageDeck_0.2.0_amd64.AppImage',
    'UsageDeck_0.2.0_arm64.AppImage',
    'UsageDeck_0.2.0_amd64.deb',
    'UsageDeck_0.2.0_arm64.deb',
    'UsageDeck_0.2.0_universal.app.tar.gz',
  ];
  const release = {
    body: 'Release notes',
    created_at: '2026-07-14T00:00:00Z',
    assets: [
      ...artifactNames.flatMap((name, index) => [
        { id: index * 2 + 1, name },
        { id: index * 2 + 2, name: `${name}.sig` },
      ]),
      { id: 99, name: 'UsageDeck_0.2.0_universal.dmg' },
    ],
  };
  const signatures = Object.fromEntries(
    artifactNames.map((name) => [`${name}.sig`, `signature:${name}`]),
  );

  it('creates deterministic updater entries for every supported architecture', () => {
    const update = createUpdaterMetadata(
      release,
      signatures,
      'lamchun1110/UsageDeck',
      'v0.2.0',
      '0.2.0',
    );

    expect(update.platforms['windows-x86_64'].url).toBe(
      'https://github.com/lamchun1110/UsageDeck/releases/download/v0.2.0/UsageDeck_0.2.0_x64-setup.exe',
    );
    expect(update.platforms['windows-aarch64'].url).toContain('_arm64-setup.exe');
    expect(update.platforms['linux-aarch64'].url).toContain('_arm64.AppImage');
    expect(update.platforms['darwin-aarch64'].url).toContain('_universal.app.tar.gz');
    expect(Object.keys(update.platforms)).toHaveLength(16);
    expect(Object.keys(update.platforms)).toEqual(
      expect.arrayContaining([
        'windows-x86_64',
        'windows-aarch64',
        'linux-x86_64',
        'linux-aarch64',
        'darwin-x86_64',
        'darwin-aarch64',
      ]),
    );
  });

  it('rejects a release whose ARM64 artifact is missing', () => {
    expect(() =>
      createUpdaterMetadata(
        {
          ...release,
          assets: release.assets.filter(
            (asset) => !asset.name.startsWith('UsageDeck_0.2.0_arm64-setup.exe'),
          ),
        },
        signatures,
        'lamchun1110/UsageDeck',
        'v0.2.0',
        '0.2.0',
      ),
    ).toThrow('Expected one Windows ARM64 NSIS installer, found 0');
  });
});
