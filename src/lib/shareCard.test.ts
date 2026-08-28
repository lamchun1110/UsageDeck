import { afterEach, describe, expect, it, vi } from 'vitest';
import { codexState, providerCatalogIndex, settingsState } from '../test/appFixtures';
import { ProviderCatalogIndex } from './metrics';
import {
  buildProviderShareRows as buildProviderShareRowsWithCatalog,
  copyShareCard,
  providerIconPlacement,
  providerShareCardHeight,
  SHARE_CARD_SCALE,
  SHARE_CARD_WIDTH,
} from './shareCard';
import type { AppSettings, ProviderLayout, ProviderSnapshot } from './types';

function buildProviderShareRows(
  _providerId: string,
  snapshot: ProviderSnapshot,
  layout: ProviderLayout,
  settings: AppSettings,
  now: number,
) {
  return buildProviderShareRowsWithCatalog(providerCatalogIndex, snapshot, layout, settings, now);
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('share card layout', () => {
  it('falls back to copying provider text when image clipboard items are unavailable', async () => {
    vi.stubGlobal('ClipboardItem', undefined);
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    const canvas = document.createElement('canvas');
    canvas.toBlob = (callback) => callback(new Blob(['png'], { type: 'image/png' }));

    await copyShareCard(canvas, 'Codex\n75% left');

    expect(writeText).toHaveBeenCalledWith('Codex\n75% left');
  });

  it('uses the authored width and 4x export scale', () => {
    expect(SHARE_CARD_WIDTH).toBe(360);
    expect(SHARE_CARD_SCALE).toBe(4);
  });

  it('aspect-fits non-100 provider viewBoxes in exported headers', () => {
    expect(providerIconPlacement('codex', 16, 16, 22)).toEqual({
      x: 16,
      y: 16,
      scale: 0.22,
    });
    expect(providerIconPlacement('devin', 16, 16, 22)).toEqual({
      x: 17.32,
      y: 16,
      scale: 0.44,
    });
    const opencode = providerIconPlacement('opencode', 16, 16, 22);
    expect(opencode.x).toBeCloseTo(18.2);
    expect(opencode.y).toBe(16);
    expect(opencode.scale).toBeCloseTo(22 / 30);
  });

  it('exports only the provider rows visible on the dashboard', () => {
    const snapshot = codexState.snapshot!;
    const settings = settingsState.settings;
    const collapsed = settings.providers[0];
    const collapsedRows = buildProviderShareRows(
      'codex',
      snapshot,
      collapsed,
      settings,
      Date.now(),
    );

    expect(collapsedRows.map((row) => row.kind)).toEqual(['quota', 'quota', 'trend']);

    const expandedRows = buildProviderShareRows(
      'codex',
      snapshot,
      { ...collapsed, expanded: true },
      settings,
      Date.now(),
    );
    expect(expandedRows.map((row) => row.kind)).toEqual([
      'quota',
      'quota',
      'trend',
      'quota',
      'quota',
      'text',
      'text',
      'text',
      'text',
      'text',
    ]);
    expect(expandedRows.slice(-3)).toMatchObject([
      { condensed: true },
      { condensed: true },
      { condensed: true },
    ]);
  });

  it('does not encode unknown pricing as an approximation prefix', () => {
    const snapshot = structuredClone(codexState.snapshot!);
    snapshot.usage.today = {
      tokens: 500,
      estimatedCostUsd: 0.03,
      costEstimated: true,
      estimateComplete: false,
      unknownModels: ['future-unpriced-model'],
    };
    const rows = buildProviderShareRows(
      'codex',
      snapshot,
      { ...settingsState.settings.providers[0], expanded: true },
      settingsState.settings,
      Date.now(),
    );

    expect(rows.find((row) => row.kind === 'text' && row.label === 'Today')).toMatchObject({
      value: '$0.03 · 500 tokens',
    });
  });

  it('keeps provider notices in exported cards', () => {
    const snapshot = structuredClone(codexState.snapshot!);
    snapshot.notices = [
      {
        id: 'rateLimited',
        title: 'Live usage paused',
        message: 'Retrying in about 5 minutes',
        tone: 'warning',
      },
    ];
    const rows = buildProviderShareRows(
      'codex',
      snapshot,
      settingsState.settings.providers[0],
      settingsState.settings,
      Date.now(),
    );

    expect(rows[0]).toMatchObject({
      kind: 'text',
      label: 'Live usage paused',
      value: 'Retrying in about 5 minutes',
    });
  });

  it('keeps provider-supplied count units in exported quota rows', () => {
    const snapshot = structuredClone(codexState.snapshot!);
    snapshot.quotas[0] = {
      ...snapshot.quotas[0],
      format: 'count',
      usedPercent: 25,
      usedValue: 25,
      limitValue: 100,
      unit: 'searches',
    };

    const rows = buildProviderShareRows(
      'codex',
      snapshot,
      settingsState.settings.providers[0],
      settingsState.settings,
      Date.now(),
    );

    expect(rows[0]).toMatchObject({ kind: 'quota', reading: '75 searches left' });
  });

  it('omits pacing copy for an unused non-session quota', () => {
    const now = Date.parse('2026-08-12T12:00:00Z');
    const snapshot = structuredClone(codexState.snapshot!);
    snapshot.quotas[1] = {
      ...snapshot.quotas[1],
      usedPercent: 0,
      resetsAt: new Date(now + (snapshot.quotas[1].periodSeconds * 1000) / 2).toISOString(),
    };
    const rows = buildProviderShareRows(
      'codex',
      snapshot,
      settingsState.settings.providers[0],
      { ...settingsState.settings, alwaysShowPacing: true },
      now,
    );

    expect(rows.find((row) => row.kind === 'quota' && row.label === 'Weekly')).toMatchObject({
      paceLabel: null,
    });
  });

  it('exports customizable status metrics as text rows', () => {
    const catalog = new ProviderCatalogIndex({
      providers: [
        {
          id: 'grok',
          displayName: 'Grok',
          shortName: 'G',
          fallbackEnabled: false,
          localUsageSourceNote: null,
          links: [],
          metrics: [
            {
              id: 'grok.payAsYouGo',
              label: 'Extra Usage',
              source: { kind: 'status', sourceId: 'payAsYouGo' },
              pinnable: true,
              defaultEnabled: true,
              defaultSection: 'alwaysVisible',
              defaultPinned: true,
              tray: { shortLabel: 'E', suffix: null },
            },
          ],
        },
      ],
    });
    const snapshot: ProviderSnapshot = {
      providerId: 'grok',
      plan: null,
      quotas: [],
      valueMetrics: [],
      statusMetrics: [
        {
          id: 'payAsYouGo',
          label: 'Extra Usage',
          text: '2500 cap',
          tone: 'positive',
        },
      ],
      notices: [],
      usage: { today: null, yesterday: null, last30Days: null, daily: [], unknownModels: [] },
      warnings: [],
      refreshedAt: '2026-07-18T00:00:00Z',
    };
    const layout: ProviderLayout = {
      id: 'grok',
      enabled: true,
      detected: true,
      expanded: false,
      metrics: [
        {
          id: 'grok.payAsYouGo',
          enabled: true,
          section: 'alwaysVisible',
          pinned: true,
        },
      ],
    };

    expect(
      buildProviderShareRowsWithCatalog(
        catalog,
        snapshot,
        layout,
        settingsState.settings,
        Date.now(),
      ),
    ).toEqual([{ kind: 'text', label: 'Extra Usage', value: '2500 cap', condensed: false }]);
  });

  it('keeps always-visible rows ahead of expanded rows like the dashboard', () => {
    const snapshot = codexState.snapshot!;
    const settings = settingsState.settings;
    const layout = settings.providers[0];
    const metric = (id: string) => layout.metrics.find((item) => item.id === id)!;
    const interleaved = {
      ...layout,
      expanded: true,
      metrics: [
        metric('codex.today'),
        metric('codex.session'),
        metric('codex.yesterday'),
        metric('codex.weekly'),
      ],
    };

    const rows = buildProviderShareRows('codex', snapshot, interleaved, settings, Date.now());
    expect(rows.map((row) => row.label)).toEqual(['Session', 'Weekly', 'Today', 'Yesterday']);
  });

  it('grows provider exports with content instead of enforcing a minimum canvas', () => {
    const settings = settingsState.settings;
    const layout = { ...settings.providers[0], expanded: true };
    const rows = buildProviderShareRows(
      'codex',
      codexState.snapshot!,
      layout,
      settings,
      Date.now(),
    );

    expect(providerShareCardHeight(rows)).toBeGreaterThan(
      providerShareCardHeight(rows.slice(0, 1)),
    );
    expect(providerShareCardHeight([])).toBeLessThan(providerShareCardHeight(rows));
  });
});
