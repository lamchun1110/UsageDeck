<script lang="ts">
  import { onDestroy } from 'svelte';
  import { flip } from 'svelte/animate';
  import { removeApiKeyAccount } from './backend';
  import { t } from './i18n.svelte';
  import { moveMetricIntoSection, reorderMetric } from './reorder';
  import type { ProviderCatalogIndex } from './metrics';
  import type { AppSettings, MetricLayout, MetricSection, ProviderLayout } from './types';
  import Icon from './Icon.svelte';
  import ProviderApiKeySection from './ProviderApiKeySection.svelte';
  import ProviderIcon from './ProviderIcon.svelte';
  import ProviderNameSection from './ProviderNameSection.svelte';
  import ProviderOptionsSection from './ProviderOptionsSection.svelte';
  import { reorderFlip } from './motion';
  import { pointerReorder } from './pointerReorder';
  import { canRenameProvider } from './providerNames';

  interface Props {
    settings: AppSettings;
    providerId: string;
    catalog: ProviderCatalogIndex;
    renamableProviderIds: string[];
    onChange: (settings: AppSettings) => void;
    onNameChange: (settings: AppSettings) => void;
    onOptionChange: (settings: AppSettings) => void;
    onReorderStart: () => void;
    onReorderEnd: (moved: boolean, cancelled?: boolean) => void;
    reducedMotion: boolean;
  }
  let {
    settings,
    providerId,
    catalog,
    renamableProviderIds,
    onChange,
    onNameChange,
    onOptionChange,
    onReorderStart,
    onReorderEnd,
    reducedMotion,
  }: Props = $props();
  const metricDefinition = (id: string) => catalog.metric(id);
  const providerDisplayName = (id: string) => catalog.displayName(id, settings.providerNames);
  let message = $state('');
  let messageKind = $state<'success' | 'denied'>('success');
  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  onDestroy(() => {
    if (messageTimer) clearTimeout(messageTimer);
  });
  const provider = $derived(settings.providers.find((item) => item.id === providerId));
  const isApiKeyAccount = $derived(
    /^[^@]+@[0-9a-f]{8}$/.test(providerId) &&
      catalog.provider(providerId) === undefined &&
      catalog.supportsApiKeyConfiguration(providerId.split('@')[0]!),
  );

  async function removeAccount() {
    if (!isApiKeyAccount) return;
    try {
      await removeApiKeyAccount(providerId);
      // The registry update lands with the next settings-state event, which
      // removes this screen; keep a brief confirmation in case it lingers.
      showMessage(t('settings.accountRemoved'), 'success');
    } catch (error) {
      showMessage((error as Error).message ?? String(error), 'denied');
    }
  }

  function updateProvider(next: ProviderLayout) {
    onChange({
      ...settings,
      providers: settings.providers.map((item) => (item.id === next.id ? next : item)),
    });
  }
  function updateMetric(metric: MetricLayout) {
    if (!provider) return;
    updateProvider({
      ...provider,
      metrics: provider.metrics.map((item) => (item.id === metric.id ? metric : item)),
    });
  }
  function togglePin(metric: MetricLayout, button: HTMLButtonElement) {
    if (!provider || !metricDefinition(metric.id)?.pinnable) return;
    if (!metric.pinned && provider.metrics.filter((item) => item.pinned).length >= 2) {
      showMessage(t('customize.starsLimit'), 'denied');
      if (!reducedMotion) {
        button.animate?.(
          [
            { transform: 'translateX(0)' },
            { transform: 'translateX(5px)' },
            { transform: 'translateX(-5px)' },
            { transform: 'translateX(5px)' },
            { transform: 'translateX(-5px)' },
            { transform: 'translateX(5px)' },
            { transform: 'translateX(0)' },
          ],
          { duration: 400, delay: 100 },
        );
      }
      return;
    }
    showMessage(metric.pinned ? t('customize.unstarred') : t('customize.starred'), 'success');
    updateMetric({ ...metric, pinned: !metric.pinned });
  }
  // Session Kickstart: this provider must publish a rolling "session"
  // window for the expiry trigger to exist.
  const kickstartEligible = $derived(catalog.hasSessionWindow(providerId));
  const hasBuiltin = $derived(catalog.supportsSessionKickstart(providerId));
  const kickstartEnabled = $derived(settings.kickstartProviderIds.includes(providerId));

  // Command edits commit on blur/Enter so typing does not save per keystroke.
  let commandDrafts = $state<Record<string, string>>({});

  function commandValue() {
    return commandDrafts[providerId] ?? (settings.kickstartCommands[providerId] ?? '');
  }

  function commitCommand() {
    if (!(providerId in commandDrafts)) return;
    const draft = (commandDrafts[providerId] ?? '').trim();
    delete commandDrafts[providerId];
    const current = settings.kickstartCommands[providerId] ?? '';
    if (draft === current) return;
    const next = { ...settings.kickstartCommands };
    if (draft) next[providerId] = draft;
    else delete next[providerId];
    onChange({ ...settings, kickstartCommands: next });
  }

  function toggleKickstart(enabled: boolean) {
    const ids = settings.kickstartProviderIds.filter((id) => id !== providerId);
    if (enabled) ids.push(providerId);
    onChange({ ...settings, kickstartProviderIds: ids.sort() });
  }

  function showMessage(text: string, kind: 'success' | 'denied') {
    message = text;
    messageKind = kind;
    if (messageTimer) clearTimeout(messageTimer);
    messageTimer = setTimeout(() => (message = ''), 2500);
  }
  function reorder(
    draggedId: string,
    target: MetricLayout,
    section: MetricSection = target.section,
  ) {
    if (!provider) return;
    const metrics = reorderMetric(provider.metrics, draggedId, target.id, section);
    if (metrics) updateProvider({ ...provider, metrics });
  }
  function moveIntoSection(draggedId: string, section: MetricSection) {
    if (!provider) return;
    const metrics = moveMetricIntoSection(provider.metrics, draggedId, section);
    if (metrics) updateProvider({ ...provider, metrics });
  }
</script>

{#if provider}
  <section
    class="screen customize-detail"
    aria-label={t('customize.detailAria', { provider: providerDisplayName(provider.id) })}
  >
    {#if canRenameProvider(provider.id, renamableProviderIds)}
      <ProviderNameSection {settings} {provider} {catalog} onChange={onNameChange} />
    {/if}
    {#each ['alwaysVisible', 'onDemand'] as section (section)}
      {@const sectionMetrics = provider.metrics.filter((metric) => metric.section === section)}
      <div
        class="metric-section"
        role="group"
        aria-label={section === 'alwaysVisible'
          ? t('customize.section.alwaysVisibleAria')
          : t('customize.section.onDemandAria')}
      >
        <h2>
          {section === 'alwaysVisible'
            ? t('customize.section.alwaysVisible')
            : t('customize.section.onDemand')}
        </h2>
        <div class="metric-list" role="list">
          {#if sectionMetrics.length === 0}
            <div
              class="empty-drop-zone"
              role="listitem"
              data-reorder-group={`customize-metrics:${provider.id}`}
              data-reorder-id={`section:${section}`}
            >
              {t('customize.dropHere')}
            </div>
          {/if}
          {#each sectionMetrics as metric (metric.id)}
            <div
              role="listitem"
              class:disabled={!metric.enabled}
              class="customize-metric-row"
              data-reorder-group={`customize-metrics:${provider.id}`}
              data-reorder-id={metric.id}
              use:pointerReorder={{
                id: metric.id,
                group: `customize-metrics:${provider.id}`,
                label: metricDefinition(metric.id)?.label ?? metric.id,
                gripOnly: true,
                touchGripOnly: true,
                onReorder: (targetId) => {
                  if (targetId.startsWith('section:')) {
                    moveIntoSection(metric.id, targetId.slice(8) as MetricSection);
                    return;
                  }
                  const target = provider.metrics.find((item) => item.id === targetId);
                  if (target) reorder(metric.id, target, target.section);
                },
                onStart: onReorderStart,
                onEnd: onReorderEnd,
              }}
              animate:flip={reorderFlip(reducedMotion)}
            >
              <span
                class="reorder-grip"
                data-reorder-handle
                data-reorder-touch-handle
                role="button"
                tabindex="0"
                aria-label={t('customize.moveAria', {
                  label: metricDefinition(metric.id)?.label ?? metric.id,
                })}
                aria-describedby="reorder-instructions"
                aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
                ><Icon name="grip-lines" size={16} strokeWidth={2} /></span
              >
              <span class="customize-metric-name"
                >{metricDefinition(metric.id)?.label ?? metric.id}</span
              >
              <span class="customize-metric-pin-slot">
                {#if metricDefinition(metric.id)?.pinnable}<button
                    class:pinned={metric.pinned}
                    class="pin-button"
                    type="button"
                    aria-label={metric.pinned
                      ? t('customize.unpinAria', {
                          metric: metricDefinition(metric.id)?.label ?? metric.id,
                        })
                      : t('customize.pinAria', {
                          metric: metricDefinition(metric.id)?.label ?? metric.id,
                        })}
                    onclick={(event) => togglePin(metric, event.currentTarget)}
                    ><Icon
                      name={metric.pinned ? 'star-filled' : 'star'}
                      size={15}
                      strokeWidth={1.7}
                    /></button
                  >{/if}
              </span>
              <label class="switch"
                ><input
                  aria-label={t('customize.showAria', {
                    metric: metricDefinition(metric.id)?.label ?? metric.id,
                  })}
                  type="checkbox"
                  checked={metric.enabled}
                  onchange={(event) =>
                    updateMetric({
                      ...metric,
                      enabled: event.currentTarget.checked,
                    })}
                /><span></span></label
              >
            </div>
          {/each}
        </div>
      </div>
    {/each}
    <ProviderOptionsSection {settings} {provider} {catalog} onChange={onOptionChange} />
    {#if kickstartEligible}
      <div class="metric-section">
        <h2>{t('settings.section.kickstart')}</h2>
        <p class="kickstart-hint">{t('settings.kickstart.hint')}</p>
        <div class="kickstart-card">
          <div class="kickstart-row">
            <span class="kickstart-id">
              <ProviderIcon providerId={provider.id} size={16} />
              <b>{providerDisplayName(provider.id)}</b>
              <small
                class="kickstart-badge"
                class:needs-command={!hasBuiltin}
              >{hasBuiltin
                ? t('settings.kickstart.builtinBadge')
                : t('settings.kickstart.needsCommandBadge')}</small>
            </span>
            <input
              type="checkbox"
              checked={kickstartEnabled}
              disabled={!hasBuiltin && !settings.kickstartCommands[provider.id]}
              aria-label={t('settings.kickstart.toggleAria', {
                provider: providerDisplayName(provider.id),
              })}
              onchange={(event) => toggleKickstart(event.currentTarget.checked)}
            />
          </div>
          <input
            class="kickstart-command"
            type="text"
            maxlength="500"
            value={commandValue()}
            placeholder={hasBuiltin && !commandValue()
              ? catalog.defaultKickstartCommand(provider.id)
              : t('settings.kickstart.customRequiredPlaceholder')}
            autocomplete="off"
            spellcheck="false"
            aria-label={t('settings.kickstart.customLabel', {
              provider: providerDisplayName(provider.id),
            })}
            oninput={(event) => (commandDrafts[provider.id] = event.currentTarget.value)}
            onblur={() => commitCommand()}
            onkeydown={(event) => {
              if (event.key === 'Enter') event.currentTarget.blur();
            }}
          />
        </div>
      </div>
    {/if}
    <ProviderApiKeySection
      providerId={provider.id}
      providerName={providerDisplayName(provider.id)}
    />
    {#if isApiKeyAccount}
      <div class="metric-section">
        <h2>{t('settings.section.account')}</h2>
        <p class="account-hint">{t('settings.account.hint')}</p>
        <button class="secondary-button account-remove" type="button" onclick={removeAccount}
          >{t('settings.account.remove')}</button
        >
      </div>
    {/if}
    {#if message}
      <div class:denied={messageKind === 'denied'} class="customization-pill" role="status">
        <Icon
          name={messageKind === 'denied' ? 'about' : 'check'}
          size={15}
          strokeWidth={2.2}
        />{message}
      </div>
    {/if}
  </section>
{/if}

<style>
  :global {
    .customize-metric-row {
      display: flex;
      min-height: 42px;
      align-items: center;
      gap: 10px;
      padding: 9px 12px;
      border-top: 1px solid var(--separator);
    }

    .customize-metric-row:first-child {
      border-top: 0;
    }

    .customize-metric-row.disabled {
      opacity: 0.55;
    }

    .customize-metric-row > label {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      gap: 5px;
      font-size: 12px;
    }

    .pin-button.pinned {
      color: var(--meter-fill);
    }

    .metric-section {
      margin-top: 0;
      margin-bottom: 14px;
    }

    .customize-metric-name {
      min-width: 0;
      flex: 1;
      overflow: hidden;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .customize-metric-pin-slot {
      display: grid;
      width: 25px;
      height: 25px;
      flex: 0 0 25px;
      place-items: center;
    }

    .customize-metric-row > .switch {
      display: block;
      flex: 0 0 28px;
    }

    .empty-drop-zone {
      display: grid;
      height: 30px;
      margin: 8px;
      border: 1px dashed var(--separator);
      border-radius: 8px;
      color: var(--tertiary);
      font-size: 10px;
      place-items: center;
    }

    .customization-pill {
      position: sticky;
      bottom: 8px;
      z-index: 20;
      display: flex;
      width: max-content;
      max-width: calc(100% - 16px);
      align-items: center;
      gap: 6px;
      margin: 8px auto 0;
      padding: 7px 10px;
      border: 1px solid var(--separator);
      border-radius: 999px;
      color: var(--text);
      background: color-mix(in srgb, var(--tray) 96%, transparent);
      box-shadow: 0 8px 22px rgba(0, 0, 0, 0.22);
      font-size: 10px;
      animation: detail-in var(--motion-spring) both;
    }

    .kickstart-hint {
      margin: 0 8px 6px;
      color: var(--secondary);
      font-size: 10px;
      line-height: 14px;
    }

    .kickstart-card {
      overflow: hidden;
      border-radius: 12px;
      background: var(--card);
    }

    .kickstart-row {
      display: flex;
      min-height: 42px;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 9px 12px;
    }

    .kickstart-id {
      display: flex;
      min-width: 0;
      align-items: center;
      gap: 7px;
      font-size: 13px;
    }

    .kickstart-id b {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .kickstart-badge {
      flex: 0 0 auto;
      padding: 1px 6px;
      border-radius: 999px;
      color: var(--secondary);
      background: var(--button-hover);
      font-size: 9px;
      font-weight: 600;
    }

    .kickstart-badge.needs-command {
      color: var(--warning);
      background: var(--warning-bg);
    }

    .kickstart-command {
      display: block;
      width: 100%;
      box-sizing: border-box;
      margin: 0;
      padding: 7px 12px 10px;
      border: 0;
      outline: none;
      color: var(--text);
      background: color-mix(in srgb, var(--card) 75%, var(--tray));
      border-top: 1px solid var(--separator);
      font-family:
        ui-monospace,
        'SF Mono',
        Menlo,
        monospace;
      font-size: 10px;
    }

    .kickstart-command::placeholder {
      color: var(--tertiary);
    }

    .kickstart-command:focus {
      box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--meter-fill) 55%, transparent);
    }

    .customization-pill .symbol-icon {
      color: var(--success);
    }

    .customization-pill.denied {
      color: var(--warning);
      animation: detail-in var(--motion-spring) both;
    }

    .customization-pill.denied .symbol-icon {
      color: var(--warning);
    }
  }
</style>
