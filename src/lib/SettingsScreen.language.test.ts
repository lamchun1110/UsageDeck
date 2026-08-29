import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import SettingsScreen from './SettingsScreen.svelte';
import type { AppSettings, SettingsViewState } from './types';

const baseSettings: AppSettings = {
  schemaVersion: 8,
  providerNames: {},
  providerOptions: {},
  knownProviderIds: [],
  providers: [],
  theme: 'system',
  accent: 'iris',
  density: 'default',
  reduceAnimations: false,
  windowMode: 'popup',
  menuBarStyle: 'text',
  usageDisplay: 'left',
  resetDisplay: 'countdown',
  timeFormat: 'system',
  language: 'system',
  alwaysShowPacing: false,
  launchAtLogin: false,
  autoCheckUpdates: true,
  dismissedUpdateVersion: null,
  lastUpdateCheckAt: null,
  globalShortcut: null,
  logLevel: 'info',
  notifications: {
    almostOut: true,
    cuttingItClose: true,
    willRunOut: true,
  },
  detectionNoticeDismissed: true,
  kickstartProviderIds: [],
};

function settingsView(overrides: Partial<AppSettings> = {}): SettingsViewState {
  return {
    settings: { ...baseSettings, ...overrides },
    settingsRevision: 1,
    accountRevision: 0,
    renamableProviderIds: [],
    notificationPermission: 'granted',
    integrationError: null,
    trayAvailable: true,
    platformSummary: null,
    keyMigrationFailedProviders: [],
  };
}

describe('SettingsScreen language selector', () => {
  afterEach(cleanup);

  it('offers the language preference and reports the selection', async () => {
    const onChange = vi.fn();
    render(SettingsScreen, {
      props: {
        settingsView: settingsView(),
        kickstartProviders: [],
        platform: 'linux',
        panelHeightMode: 'automatic',
        onChange,
        onPanelHeightModeChange: vi.fn(),
        onRequestNotifications: vi.fn(),
        onOpenNotificationSettings: vi.fn(),
        updateError: null,
        checkingUpdate: false,
        onCheckForUpdates: vi.fn(),
        onCustomize: vi.fn(),
        onCopyLogPath: vi.fn(),
        onOpenLogFolder: vi.fn(),
        onResetAllSettings: vi.fn(),
      },
    });

    await fireEvent.click(screen.getByRole('combobox', { name: 'Language' }));
    await fireEvent.click(screen.getByRole('option', { name: '简体中文' }));

    expect(onChange).toHaveBeenCalledTimes(1);
    const updated = onChange.mock.calls[0][0] as AppSettings;
    expect(updated.language).toBe('zh-CN');
  });
});
