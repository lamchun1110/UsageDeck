import { describe, expect, it } from 'vitest';
import layoutCss from '../styles/layout.css?raw';
import sharedComponentCss from '../styles/components.css?raw';
import tokensCss from '../styles/tokens.css?raw';
import customizeDetail from './CustomizeProviderDetail.svelte?raw';
import customizeList from './CustomizeProviderList.svelte?raw';
import dashboard from './Dashboard.svelte?raw';
import enMessages from './messages/en?raw';
import providerNameSection from './ProviderNameSection.svelte?raw';
import settings from './SettingsScreen.svelte?raw';
import { coLocatedComponentCss } from './uiStyleSources';

const css = `${tokensCss}\n${layoutCss}\n${sharedComponentCss}\n${coLocatedComponentCss}`;

describe('native UI language contract', () => {
  it('uses the platform system font and reference type sizes', () => {
    expect(css).toMatch(/font-family:\s*system-ui,/);
    expect(css).not.toMatch(/font-family:\s*Inter/);
    expect(css).toMatch(/\.provider-header h1\s*{[^}]*font-size: 14px;[^}]*font-weight: 600;/s);
    expect(css).toMatch(/\.provider-list-main b\s*{[^}]*font-size: 14px;[^}]*font-weight: 600;/s);
    expect(css).toMatch(/\.setting-row\s*{[^}]*font-size: 13px;/s);
  });

  it('keeps the critical flame colored while its warning copy stays secondary', () => {
    expect(css).not.toMatch(/\.metric__heading span\s*{/);
    expect(css).toMatch(
      /\.metric__heading \.pace-warning__icon\s*{[^}]*color: var\(--meter-critical\);/s,
    );
    expect(css).toMatch(/\.metric__heading \.pace-warning\s*{[^}]*color: var\(--secondary\);/s);
  });

  it('keeps spend providers visually distinct in both appearances', () => {
    for (const provider of ['claude', 'codex', 'cursor', 'grok', 'opencode', 'openrouter']) {
      expect(tokensCss).toContain(`--provider-${provider}:`);
    }
    expect(tokensCss).toMatch(
      /@media \(prefers-color-scheme: dark\)[\s\S]*--provider-cursor: #f5f5f7;[\s\S]*--provider-opencode: #aeaeb2;/,
    );
    expect(tokensCss).toMatch(
      /:root\[data-theme='dark'\][\s\S]*--provider-cursor: #f5f5f7;[\s\S]*--provider-opencode: #aeaeb2;/,
    );
  });

  it('keeps Customize concise and free of duplicate status and count copy', () => {
    expect(customizeList).toContain('Notifications, appearance and more');
    expect(customizeList).toContain('{provider.metrics.length} metrics');
    expect(customizeList).not.toContain('Detected locally');
    expect(customizeList).not.toContain('screen-intro');
    expect(customizeList).not.toContain('pinned\n');
    expect(customizeDetail).toContain('Drag metrics here');
    expect(customizeDetail).toContain("t('customize.starred')");
    expect(enMessages).toContain("'customize.starred': 'Starred for menu bar'");
    expect(customizeDetail).toContain("t('customize.unstarred')");
    expect(enMessages).toContain("'customize.unstarred': 'Removed from menu bar'");
    expect(customizeDetail).toContain("t('customize.starsLimit')");
    expect(enMessages).toContain("'customize.starsLimit': 'Up to 2 stars per provider'");
    expect(customizeDetail).not.toContain('provider-toggle-row');
    expect(customizeDetail).not.toContain('section-divider');
    expect(customizeDetail).not.toContain('of 2 pinned');
  });

  it('uses the shared Settings labels and single-line control rows', () => {
    // Labels live in the English message catalog; the screen consumes them through t().
    for (const label of [
      'General',
      'Launch at Login',
      'Global Shortcut',
      'Icon Style',
      'Appearance',
      'Window Mode',
      'Usage Display',
      'Notifications',
      'Advanced',
      'Updates',
      'Check for Updates Automatically',
      'Check for Updates…',
      'Auto',
      '12-hour',
      '24-hour',
    ]) {
      expect(enMessages).toContain(label);
    }
    for (const key of [
      "t('settings.section.general')",
      "t('settings.row.launchAtLogin')",
      "t('settings.row.iconStyle')",
      "t('settings.row.windowMode')",
      "t('settings.row.autoCheckUpdates')",
      "t('settings.option.auto')",
      "t('settings.option.twelveHour')",
      "t('settings.option.twentyFourHour')",
    ]) {
      expect(settings).toContain(key);
    }
    expect(settings).not.toContain('<h2>Startup</h2>');
    expect(settings).not.toContain('Automatic Checks');
    expect(settings).not.toContain('Combined cost and token summary.');
    expect(settings).not.toContain('Show projections even when usage is healthy.');
    expect(settings).not.toContain('>×</button');
  });

  it('keeps dashboard onboarding, empty state, and menus on the shared wording', () => {
    expect(dashboard).toContain("t('dashboard.welcome.title')");
    expect(dashboard).toContain("t('dashboard.welcome.openCustomize')");
    expect(dashboard).toContain("t('dashboard.empty')");
    expect(dashboard).toContain("t('dashboard.menu.customize')");
    expect(dashboard).toContain("t('dashboard.menu.refreshProvider', {");
    // The English catalog carries the canonical wording these surfaces show.
    expect(enMessages).toContain("'dashboard.welcome.title': 'Welcome to UsageDeck'");
    expect(enMessages).toContain("'dashboard.welcome.openCustomize': 'Open Customize'");
    expect(enMessages).toContain("'dashboard.empty': 'Turn on Customize to choose what to show.'");
    expect(enMessages).toContain("'dashboard.menu.customize': 'Customize…'");
    expect(dashboard).not.toContain('Providers Detected');
    expect(dashboard).not.toContain('Starter Provider');
    expect(dashboard).not.toContain("Expand'} On Demand");
    expect(dashboard).not.toContain('>×</button');
  });

  it('keeps interactive highlights in the component layer that owns their base style', () => {
    expect(providerNameSection).toMatch(
      /\.provider-name-card:focus-within\s*{[^}]*box-shadow: inset 0 0 0 2px/s,
    );
    expect(providerNameSection).toMatch(/input\s*{[^}]*display: block;/s);
    expect(dashboard).toMatch(
      /\.context-menu button:not\(:disabled\):hover,[\s\S]*background: var\(--button-hover\);/,
    );
    expect(sharedComponentCss).not.toContain('.context-menu button:hover');
  });
});
