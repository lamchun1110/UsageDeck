<script lang="ts">
  import type { PanelHeightMode } from './backend';
  import { LANGUAGE_PREFERENCES, t } from './i18n.svelte';
  import Icon from './Icon.svelte';
  import type { DesktopPlatform } from './platform';
  import SelectMenu from './SelectMenu.svelte';
  import type {
    AppSettings,
    NotificationPreferences,
    SettingsViewState,
    UpdateFailure,
  } from './types';

  interface Props {
    settingsView: SettingsViewState;
    platform: DesktopPlatform;
    panelHeightMode: PanelHeightMode;
    onChange: (settings: AppSettings) => void;
    onPanelHeightModeChange: (mode: PanelHeightMode) => void;
    onRequestNotifications: () => void;
    onOpenNotificationSettings: () => void;
    updateError: UpdateFailure | null;
    checkingUpdate: boolean;
    onCheckForUpdates: () => void;
    onCustomize: () => void;
    onCopyLogPath: () => Promise<void>;
    onOpenLogFolder: () => Promise<void>;
    onResetAllSettings: () => void;
  }
  let {
    settingsView,
    platform,
    panelHeightMode,
    onChange,
    onPanelHeightModeChange,
    onRequestNotifications,
    onOpenNotificationSettings,
    updateError,
    checkingUpdate,
    onCheckForUpdates,
    onCustomize,
    onCopyLogPath,
    onOpenLogFolder,
    onResetAllSettings,
  }: Props = $props();
  let recording = $state(false);
  let logActionError = $state<string | null>(null);
  const settings = $derived(settingsView.settings);
  const revealLogLabel = $derived(
    platform === 'macos'
      ? t('settings.btn.revealMac')
      : platform === 'windows'
        ? t('settings.btn.revealWindows')
        : t('settings.btn.revealLinux'),
  );
  const anyNotificationEnabled = $derived(
    settings.notifications.almostOut ||
      settings.notifications.cuttingItClose ||
      settings.notifications.willRunOut,
  );
  const notificationsNeedAttention = $derived(
    anyNotificationEnabled && settingsView.notificationPermission !== 'granted',
  );

  function patch(value: Partial<AppSettings>) {
    onChange({ ...settings, ...value });
  }
  function patchNotification(key: keyof NotificationPreferences, enabled: boolean) {
    patch({ notifications: { ...settings.notifications, [key]: enabled } });
    if (enabled && settingsView.notificationPermission === 'prompt') onRequestNotifications();
  }
  async function copyLogPath() {
    try {
      await onCopyLogPath();
      logActionError = null;
    } catch {
      logActionError = t('settings.log.copyError');
    }
  }
  async function revealLogFile() {
    try {
      await onOpenLogFolder();
      logActionError = null;
    } catch {
      logActionError = t('settings.log.revealError');
    }
  }
  function record(event: KeyboardEvent) {
    if (!recording) return;
    if (event.key === 'Tab') {
      recording = false;
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Escape') {
      recording = false;
      return;
    }
    if (event.key === 'Delete' || event.key === 'Backspace') {
      patch({ globalShortcut: null });
      recording = false;
      return;
    }
    if (
      !(event.ctrlKey || event.altKey || event.metaKey) ||
      ['Control', 'Alt', 'Meta', 'Shift'].includes(event.key)
    )
      return;
    const modifiers = [
      event.ctrlKey && 'Ctrl',
      event.altKey && 'Alt',
      event.shiftKey && 'Shift',
      event.metaKey && 'Super',
    ].filter(Boolean);
    const key = event.code.startsWith('Key')
      ? event.code.slice(3)
      : event.code.startsWith('Digit')
        ? event.code.slice(5)
        : event.key.length === 1
          ? event.key.toUpperCase()
          : event.key;
    patch({ globalShortcut: [...modifiers, key].join('+') });
    recording = false;
  }
</script>

<section class="screen settings-screen" aria-label={t('settings.aria.settings')}>
  {#if settingsView.integrationError}<p class="notice" role="alert">
      {settingsView.integrationError}
    </p>{/if}

  {#if settingsView.platformSummary}<div class="settings-section">
      <h2>Linux</h2>
      <div class="setting-row">
        <span
          ><b>{t('settings.row.desktopIntegration')}</b><small>{settingsView.platformSummary}</small
          ></span
        >
      </div>
    </div>{/if}

  <div class="settings-section">
    <h2>{t('settings.section.general')}</h2>
    <label class="setting-row"
      ><span><b>{t('settings.row.launchAtLogin')}</b></span><input
        type="checkbox"
        checked={settings.launchAtLogin}
        onchange={(event) => patch({ launchAtLogin: event.currentTarget.checked })}
      /></label
    >
    <div class="setting-row">
      <span><b>{t('settings.row.globalShortcut')}</b></span>
      <div class="shortcut-field">
        <button
          class:recording
          type="button"
          aria-pressed={recording}
          aria-describedby="shortcut-recording-help"
          data-tooltip={t('settings.shortcut.tooltip')}
          onclick={() => (recording = !recording)}
          onkeydown={record}
          onblur={() => (recording = false)}
          >{recording
            ? t('settings.shortcut.type')
            : (settings.globalShortcut ?? t('settings.shortcut.record'))}</button
        >{#if settings.globalShortcut}<button
            class="shortcut-clear"
            type="button"
            aria-label={t('settings.shortcut.clear')}
            onclick={() => patch({ globalShortcut: null })}
            ><Icon name="close" size={10} strokeWidth={2.2} /></button
          >{/if}
      </div>
      <small id="shortcut-recording-help" class="sr-only">{t('settings.shortcut.help')}</small>
    </div>
  </div>

  <div class="settings-section">
    <h2>{t('settings.section.appearance')}</h2>
    {#if platform === 'macos' || platform === 'linux'}
      <div class="setting-row">
        <span><b>{t('settings.row.iconStyle')}</b></span><SelectMenu
          label={t('settings.row.iconStyle')}
          value={settings.menuBarStyle}
          options={[
            { value: 'text', label: t('settings.option.text') },
            { value: 'bars', label: t('settings.option.bars') },
          ]}
          onChange={(value) => patch({ menuBarStyle: value as AppSettings['menuBarStyle'] })}
        />
      </div>
      {#if settings.menuBarStyle === 'bars'}
        <small class="setting-hint"
          >{t('settings.barsHint')}{#if platform === 'macos'}
            {t('settings.barsHoverHint')}{/if}</small
        >
      {/if}
    {/if}
    <div class="setting-row">
      <span><b>{t('settings.row.theme')}</b></span><SelectMenu
        label={t('settings.row.theme')}
        value={settings.theme}
        options={[
          { value: 'system', label: t('settings.option.system') },
          { value: 'light', label: t('settings.option.light') },
          { value: 'dark', label: t('settings.option.dark') },
        ]}
        onChange={(value) => patch({ theme: value as AppSettings['theme'] })}
      />
    </div>
    <div class="setting-row">
      <span><b>{t('settings.row.accent')}</b></span><SelectMenu
        label={t('settings.row.accent')}
        value={settings.accent}
        options={[
          { value: 'iris', label: t('settings.option.iris') },
          { value: 'ocean', label: t('settings.option.ocean') },
          { value: 'forest', label: t('settings.option.forest') },
          { value: 'rose', label: t('settings.option.rose') },
          { value: 'amber', label: t('settings.option.amber') },
        ]}
        onChange={(value) => patch({ accent: value as AppSettings['accent'] })}
      />
    </div>
    <div class="setting-row">
      <span><b>{t('settings.row.density')}</b></span><SelectMenu
        label={t('settings.row.density')}
        value={settings.density}
        options={[
          { value: 'default', label: t('settings.option.default') },
          { value: 'compact', label: t('settings.option.compact') },
        ]}
        onChange={(value) => patch({ density: value as AppSettings['density'] })}
      />
    </div>
    <label class="setting-row"
      ><span><b>{t('settings.row.reduceAnimations')}</b></span><input
        type="checkbox"
        checked={settings.reduceAnimations}
        onchange={(event) => patch({ reduceAnimations: event.currentTarget.checked })}
      /></label
    >
    {#if settingsView.trayAvailable}
      <div class="setting-row">
        <span><b>{t('settings.row.windowMode')}</b></span><SelectMenu
          label={t('settings.row.windowMode')}
          value={settings.windowMode}
          options={[
            { value: 'popup', label: t('settings.option.trayPopup') },
            { value: 'floating', label: t('settings.option.floatingWindow') },
          ]}
          onChange={(value) => patch({ windowMode: value as AppSettings['windowMode'] })}
        />
      </div>
    {/if}
    <div class="setting-row">
      <span><b>{t('settings.row.panelHeight')}</b></span><SelectMenu
        label={t('settings.row.panelHeight')}
        value={panelHeightMode}
        options={[
          { value: 'automatic', label: t('settings.option.automatic') },
          { value: 'manual', label: t('settings.option.manual') },
        ]}
        onChange={(value) => onPanelHeightModeChange(value as PanelHeightMode)}
      />
    </div>
    <div class="setting-row">
      <span><b>{t('settings.row.timeFormat')}</b></span><SelectMenu
        label={t('settings.row.timeFormat')}
        value={settings.timeFormat}
        options={[
          { value: 'system', label: t('settings.option.auto') },
          { value: 'twelveHour', label: t('settings.option.twelveHour') },
          { value: 'twentyFourHour', label: t('settings.option.twentyFourHour') },
        ]}
        onChange={(value) => patch({ timeFormat: value as AppSettings['timeFormat'] })}
      />
    </div>
    <div class="setting-row">
      <span><b>{t('settings.row.language')}</b></span><SelectMenu
        label={t('settings.row.language')}
        value={settings.language}
        options={LANGUAGE_PREFERENCES.map(({ value, label }) => ({ value, label }))}
        onChange={(value) => patch({ language: value as AppSettings['language'] })}
      />
    </div>
  </div>

  <div class="settings-section">
    <h2>{t('settings.section.usageDisplay')}</h2>
    <div class="setting-row">
      <span><b>{t('settings.row.showUsageAs')}</b></span><SelectMenu
        label={t('settings.row.showUsageAs')}
        value={settings.usageDisplay}
        options={[
          { value: 'left', label: t('settings.option.left') },
          { value: 'used', label: t('settings.option.used') },
        ]}
        onChange={(value) => patch({ usageDisplay: value as AppSettings['usageDisplay'] })}
      />
    </div>
    <div class="setting-row">
      <span><b>{t('settings.row.resetTimes')}</b></span><SelectMenu
        label={t('settings.row.resetTimes')}
        value={settings.resetDisplay}
        options={[
          { value: 'countdown', label: t('settings.option.countdown') },
          { value: 'exact', label: t('settings.option.exactTime') },
        ]}
        onChange={(value) => patch({ resetDisplay: value as AppSettings['resetDisplay'] })}
      />
    </div>
    <label class="setting-row"
      ><span
        ><b>{t('settings.row.alwaysShowPacing')}</b><i
          class="setting-info"
          data-tooltip={t('settings.pacing.tooltip')}
          aria-label={t('settings.pacing.tooltip')}
          ><Icon name="about" size={12} strokeWidth={1.8} /></i
        ></span
      ><input
        type="checkbox"
        checked={settings.alwaysShowPacing}
        onchange={(event) => patch({ alwaysShowPacing: event.currentTarget.checked })}
      /></label
    >
  </div>

  <div class="settings-section">
    <h2>
      {t('settings.section.notifications')}
      {#if notificationsNeedAttention}<span class="permission-warning">!</span>{/if}
    </h2>
    <label class="setting-row"
      ><span
        ><b>{t('settings.row.almostOut')}</b><i
          class="setting-info"
          data-tooltip={t('settings.notify.almostOut.tooltip')}
          aria-label={t('settings.notify.almostOut.tooltip')}
          ><Icon name="about" size={12} strokeWidth={1.8} /></i
        ></span
      ><input
        type="checkbox"
        checked={settings.notifications.almostOut}
        onchange={(event) => patchNotification('almostOut', event.currentTarget.checked)}
      /></label
    >
    <label class="setting-row"
      ><span
        ><b>{t('settings.row.cuttingItClose')}</b><i
          class="setting-info"
          data-tooltip={t('settings.notify.cuttingItClose.tooltip')}
          aria-label={t('settings.notify.cuttingItClose.tooltip')}
          ><Icon name="about" size={12} strokeWidth={1.8} /></i
        ></span
      ><input
        type="checkbox"
        checked={settings.notifications.cuttingItClose}
        onchange={(event) => patchNotification('cuttingItClose', event.currentTarget.checked)}
      /></label
    >
    <label class="setting-row"
      ><span
        ><b>{t('settings.row.willRunOut')}</b><i
          class="setting-info"
          data-tooltip={t('settings.notify.willRunOut.tooltip')}
          aria-label={t('settings.notify.willRunOut.tooltip')}
          ><Icon name="about" size={12} strokeWidth={1.8} /></i
        ></span
      ><input
        type="checkbox"
        checked={settings.notifications.willRunOut}
        onchange={(event) => patchNotification('willRunOut', event.currentTarget.checked)}
      /></label
    >
    {#if notificationsNeedAttention}
      <div class="notification-actions">
        <div class="notification-attention" role="status">
          <span
            ><b
              >{settingsView.notificationPermission === 'denied'
                ? t('settings.notify.blocked')
                : t('settings.notify.permissionRequired')}</b
            ><small
              >{settingsView.notificationPermission === 'denied'
                ? t('settings.notify.blockedHelp')
                : t('settings.notify.permissionHelp')}</small
            ></span
          >
          <button
            class="secondary-button"
            type="button"
            onclick={settingsView.notificationPermission === 'denied'
              ? onOpenNotificationSettings
              : onRequestNotifications}
            >{settingsView.notificationPermission === 'denied'
              ? t('settings.notify.openSettings')
              : t('settings.notify.allow')}</button
          >
        </div>
      </div>
    {/if}
  </div>

  <div class="settings-section">
    <h2>{t('settings.section.advanced')}</h2>
    <div class="setting-row">
      <span><b>{t('settings.row.logLevel')}</b></span><SelectMenu
        label={t('settings.row.logLevel')}
        value={settings.logLevel}
        options={[
          { value: 'error', label: t('settings.option.error') },
          { value: 'warn', label: t('settings.option.warning') },
          { value: 'info', label: t('settings.option.info') },
          { value: 'debug', label: t('settings.option.debug') },
        ]}
        onChange={(value) => patch({ logLevel: value as AppSettings['logLevel'] })}
      />
    </div>
    <div class="setting-row setting-row--button">
      <button class="secondary-button settings-wide-button" type="button" onclick={copyLogPath}
        >{t('settings.btn.copyLogPath')}</button
      >
    </div>
    <div class="setting-row setting-row--button">
      <button class="secondary-button settings-wide-button" type="button" onclick={revealLogFile}
        >{revealLogLabel}</button
      >
    </div>
    {#if logActionError}<p class="settings-note log-action-error" role="alert">
        {logActionError}
      </p>{/if}
    <div class="setting-row setting-row--button">
      <button
        class="secondary-button settings-wide-button settings-reset-button"
        type="button"
        onclick={onResetAllSettings}>{t('settings.btn.resetAll')}</button
      >
    </div>
  </div>

  <div class="settings-section">
    <h2>{t('settings.section.updates')}</h2>
    <label class="setting-row"
      ><span><b>{t('settings.row.autoCheckUpdates')}</b></span><input
        type="checkbox"
        checked={settings.autoCheckUpdates}
        onchange={(event) => patch({ autoCheckUpdates: event.currentTarget.checked })}
      /></label
    >
    <div class="setting-row setting-row--button">
      <button
        type="button"
        class="secondary-button settings-wide-button"
        disabled={checkingUpdate}
        onclick={onCheckForUpdates}
        >{checkingUpdate ? t('settings.btn.checking') : t('settings.btn.checkUpdates')}</button
      >
    </div>
    {#if updateError}<div class="settings-update-error" role="alert">
        <b>{updateError.message}</b><small>{updateError.action}</small>
      </div>{/if}
  </div>

  <button
    class="screen-cross-link"
    type="button"
    aria-label={t('settings.customize.title')}
    onclick={onCustomize}
  >
    <Icon name="sliders" size={17} />
    <span
      ><b>{t('settings.customize.title')}</b><small>{t('settings.customize.tagline')}</small></span
    >
    <Icon name="chevron-right" size={13} strokeWidth={2.2} />
  </button>
</section>

<style>
  :global {
    .settings-section {
      margin-bottom: 10px;
    }

    .setting-row {
      display: flex;
      min-height: 40px;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 6px 10px;
      border-top: 1px solid var(--separator);
      font-size: 11px;
    }

    .settings-section h2 + .setting-row {
      border-top: 0;
    }

    .setting-row > span {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 1px;
    }

    .setting-row b {
      font-weight: 550;
    }

    .setting-row small {
      color: var(--secondary);
      font-size: 9px;
      line-height: 12px;
    }

    .setting-hint {
      display: block;
      margin: -2px 10px 6px;
      color: var(--secondary);
      font-size: 10px;
      line-height: 13px;
    }

    input[type='checkbox'] {
      width: 15px;
      height: 15px;
      accent-color: var(--meter-fill);
    }

    input[type='checkbox']:focus-visible {
      outline: 2px solid var(--meter-fill);
      outline-offset: 2px;
    }

    .settings-reset-button {
      color: var(--error);
    }

    .shortcut-field {
      display: flex;
      align-items: center;
      gap: 3px;
    }

    .shortcut-field button {
      max-width: 115px;
      padding: 4px 7px;
      overflow: hidden;
      border: 1px solid var(--separator);
      border-radius: 6px;
      color: var(--secondary);
      background: var(--tray);
      font-family: ui-monospace, monospace;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .shortcut-field button.recording {
      border-color: var(--meter-fill);
      color: var(--text);
    }

    .shortcut-field button.shortcut-clear {
      display: grid;
      width: 24px;
      height: 24px;
      padding: 0;
      color: var(--secondary);
      font-family: inherit;
      place-items: center;
    }

    .shortcut-field button.shortcut-clear:hover,
    .shortcut-field button.shortcut-clear:focus-visible {
      outline: none;
      color: var(--text);
      background: var(--button-hover);
    }

    .secondary-button {
      flex: 0 0 auto;
      padding: 4px 8px;
      border: 1px solid var(--separator);
      border-radius: 6px;
      color: var(--text);
      background: var(--tray);
      font-size: 12px;
      font-weight: 500;
    }

    .secondary-button:disabled {
      opacity: 0.55;
    }

    .permission-warning {
      display: inline-grid;
      width: 13px;
      height: 13px;
      margin-left: 3px;
      border-radius: 50%;
      color: var(--on-fill);
      background: var(--warning);
      font-size: 8px;
      place-items: center;
    }

    .settings-note,
    .version-row {
      margin: 0;
      padding: 6px 10px 9px;
      color: var(--warning);
      font-size: 9px;
    }

    .version-row {
      padding: 3px 0 8px;
      color: var(--tertiary);
      text-align: center;
    }

    .settings-section {
      margin-bottom: 14px;
      overflow: visible;
      background: transparent;
    }

    .settings-section > .setting-row {
      border-top: 0;
      background: var(--card);
    }

    .settings-section > h2 + .setting-row {
      border-radius: 12px 12px 0 0;
    }

    .settings-section > .setting-row:last-child,
    .settings-section > .settings-note:last-child {
      border-radius: 0 0 12px 12px;
    }

    .settings-section > h2 + .setting-row:last-child {
      border-radius: 12px;
    }

    .setting-row {
      min-height: 40px;
      padding: 9px 12px;
      border: 0;
      font-size: 13px;
    }

    .setting-row b {
      font-weight: 400;
    }

    .setting-row .select-menu__trigger {
      font-size: 13px;
    }

    .setting-row small {
      font-size: 10px;
      line-height: 12px;
    }

    input[type='checkbox'] {
      width: 28px;
      height: 16px;
      flex: 0 0 auto;
      margin: 0;
      appearance: none;
      border-radius: 9px;
      background: var(--meter-track);
      cursor: pointer;
      transition: background-color 160ms ease;
    }

    input[type='checkbox']::after {
      display: block;
      width: 12px;
      height: 12px;
      margin: 2px;
      border-radius: 50%;
      background: white;
      box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
      content: '';
      transition: transform 160ms ease;
    }

    input[type='checkbox']:checked {
      background: var(--meter-fill);
    }

    input[type='checkbox']:checked::after {
      transform: translateX(12px);
    }

    .version-row {
      font-size: 10px;
    }

    .setting-row > span:has(.setting-info) {
      align-items: center;
      flex-direction: row;
      gap: 6px;
    }

    .setting-info {
      display: inline-grid;
      flex: 0 0 auto;
      color: var(--secondary);
      font-style: normal;
      place-items: center;
    }

    .setting-row--button {
      display: block;
    }

    .settings-wide-button {
      width: 100%;
      min-height: 28px;
      font-size: 12px;
    }

    .settings-note.log-action-error {
      background: var(--card);
    }

    .notification-actions {
      padding: 8px 12px 10px;
      border-top: 1px solid var(--separator);
      border-radius: 0 0 12px 12px;
      background: var(--card);
    }

    .notification-attention {
      display: flex;
      align-items: center;
      gap: 10px;
      color: var(--warning);
    }

    .notification-attention > span {
      display: flex;
      min-width: 0;
      flex: 1;
      flex-direction: column;
      gap: 2px;
    }

    .notification-attention b {
      font-size: 11px;
      font-weight: 600;
      line-height: 13px;
    }

    .notification-attention small {
      color: var(--secondary);
      font-size: 9px;
      line-height: 12px;
    }

    .notification-attention .secondary-button {
      flex: 0 0 auto;
    }

    .settings-update-error {
      display: flex;
      flex-direction: column;
      gap: 2px;
      margin: 0 12px 8px;
      padding: 8px;
      border-radius: 8px;
      color: var(--error);
      background: var(--error-bg);
    }

    .settings-update-error b {
      font-size: 11px;
      line-height: 14px;
    }

    .settings-update-error small {
      color: var(--error);
      font-size: 9px;
      line-height: 12px;
    }

    :root[data-density='compact'] .setting-row {
      gap: 8px;
      padding-right: 10px;
      padding-left: 10px;
    }

    :root[data-density='compact'] .screen-cross-link {
      min-height: 42px;
      margin-top: 8px;
    }
  }
</style>
