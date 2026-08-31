<script lang="ts">
  import { flip } from 'svelte/animate';
  import { addApiKeyAccount } from './backend';
  import { t } from './i18n.svelte';
  import { reorderProviders } from './reorder';
  import type { AppSettings, ProviderLayout } from './types';
  import type { ProviderCatalogIndex } from './metrics';
  import Icon from './Icon.svelte';
  import ProviderIcon from './ProviderIcon.svelte';
  import { reorderFlip } from './motion';
  import { pointerReorder } from './pointerReorder';

  interface Props {
    settings: AppSettings;
    catalog: ProviderCatalogIndex;
    onOpen: (providerId: string) => void;
    onChange: (settings: AppSettings) => void;
    onReorderStart: () => void;
    onReorderEnd: (moved: boolean, cancelled?: boolean) => void;
    onSettings: () => void;
    reducedMotion: boolean;
  }
  let {
    settings,
    catalog,
    onOpen,
    onChange,
    onReorderStart,
    onReorderEnd,
    onSettings,
    reducedMotion,
  }: Props = $props();
  const providerDisplayName = (id: string) => catalog.displayName(id, settings.providerNames);
  // Families only: existing accounts (e.g. `kimi@1a2b3c4d`) also support API-key
  // configuration but must not be offered as families for creating new accounts.
  const apiKeyFamilies = $derived(
    catalog.providers
      .filter(
        (provider) =>
          catalog.supportsApiKeyConfiguration(provider.id) && !catalog.isApiKeyAccount(provider.id),
      )
      .map((provider) => provider.id),
  );
  let newAccountName = $state('');
  let newAccountError = $state<string | null>(null);
  let activeAccountFamily = $state<string | null>(null);
  let addingAccount = $state(false);

  function updateProvider(provider: ProviderLayout) {
    onChange({
      ...settings,
      providers: settings.providers.map((item) => (item.id === provider.id ? provider : item)),
    });
  }
  function reorder(draggedId: string, targetId: string) {
    const providers = reorderProviders(settings.providers, draggedId, targetId);
    if (providers) onChange({ ...settings, providers });
  }

  async function confirmAddAccount() {
    if (!activeAccountFamily || addingAccount) return;
    const name = newAccountName.trim();
    if (!name) {
      newAccountError = t('settings.account.nameRequired');
      return;
    }
    addingAccount = true;
    try {
      await addApiKeyAccount(activeAccountFamily, name);
      newAccountName = '';
      newAccountError = null;
      activeAccountFamily = null;
    } catch (error) {
      newAccountError = (error as Error).message ?? String(error);
    } finally {
      addingAccount = false;
    }
  }
</script>

<section class="screen customize-screen" aria-label={t('app.title.customize')}>
  <p class="account-add-hint">{t('settings.account.addHint')}</p>
  <div class="account-add-row">
    <select
      value={activeAccountFamily ?? ''}
      aria-label={t('settings.account.familyAria')}
      disabled={addingAccount}
      onchange={(e) => (activeAccountFamily = (e.currentTarget as HTMLSelectElement).value || null)}
    >
      <option value="">{t('settings.account.chooseFamily')}</option>
      {#each apiKeyFamilies as family (family)}
        <option value={family}>{providerDisplayName(family)}</option>
      {/each}
    </select>
    {#if activeAccountFamily}
      <input
        placeholder={t('settings.account.namePlaceholder')}
        value={newAccountName}
        disabled={addingAccount}
        oninput={(e) => (newAccountName = (e.currentTarget as HTMLInputElement).value)}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            void confirmAddAccount();
          }
        }}
      />
      <button
        type="button"
        class="secondary-button"
        disabled={addingAccount}
        onclick={confirmAddAccount}>{t('settings.account.add')}</button
      >
    {/if}
  </div>
  {#if newAccountError}<p class="account-error" role="alert">{newAccountError}</p>{/if}
  <div class="customize-list" role="list">
    {#each settings.providers.filter( (provider) => catalog.provider(provider.id) ) as provider (provider.id)}
      <div
        role="listitem"
        class:inactive={!provider.enabled}
        class="provider-list-row"
        data-reorder-group={provider.enabled ? 'customize-providers' : undefined}
        data-reorder-id={provider.enabled ? provider.id : undefined}
        use:pointerReorder={{
          id: provider.id,
          group: 'customize-providers',
          label: providerDisplayName(provider.id),
          disabled: !provider.enabled,
          gripOnly: true,
          touchGripOnly: true,
          onReorder: (targetId) => reorder(provider.id, targetId),
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
          tabindex={provider.enabled ? 0 : undefined}
          aria-label={t('dashboard.provider.moveHandle', {
            provider: providerDisplayName(provider.id),
          })}
          aria-describedby="reorder-instructions"
          aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
          ><Icon name="grip-lines" size={16} strokeWidth={2} /></span
        >
        <button class="provider-list-main" type="button" onclick={() => onOpen(provider.id)}
          ><ProviderIcon providerId={provider.id} /><span
            ><b>{providerDisplayName(provider.id)}</b><small
              >{t('customize.metricsCount', { count: provider.metrics.length })}</small
            ></span
          ></button
        >
        <label class="switch"
          ><input
            aria-label={t('customize.enableAria', {
              provider: providerDisplayName(provider.id),
            })}
            type="checkbox"
            checked={provider.enabled}
            onchange={(event) =>
              updateProvider({ ...provider, enabled: event.currentTarget.checked })}
          /><span></span></label
        >
        <button
          class="chevron"
          type="button"
          aria-label={t('customize.providerAria', {
            provider: providerDisplayName(provider.id),
          })}
          onclick={() => onOpen(provider.id)}
          ><Icon name="chevron-right" size={13} strokeWidth={2.2} /></button
        >
      </div>
    {/each}
  </div>
  <button
    class="screen-cross-link"
    type="button"
    aria-label={t('settings.aria.settings')}
    onclick={onSettings}
  >
    <Icon name="gear" size={17} />
    <span><b>{t('customize.openSettings')}</b><small>{t('customize.openSettingsHint')}</small></span
    >
    <Icon name="chevron-right" size={13} strokeWidth={2.2} />
  </button>
</section>

<style>
  :global {
    .provider-list-row {
      display: flex;
      min-height: 52px;
      align-items: center;
      gap: 5px;
      padding: 5px 7px;
      border-top: 1px solid var(--separator);
    }

    .provider-list-row:first-child {
      border-top: 0;
    }

    .provider-list-row.inactive {
      opacity: 0.55;
    }

    .reorder-grip {
      position: relative;
      color: var(--tertiary);
      cursor: grab;
      font-size: 16px;
    }

    .reorder-grip::after {
      position: absolute;
      inset: -10px -8px;
      content: '';
    }

    .provider-list-main {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      flex-direction: row;
      gap: 10px;
      padding: 4px;
      border: 0;
      color: var(--text);
      background: none;
      text-align: left;
    }

    .provider-list-main > span {
      display: flex;
      min-width: 0;
      flex-direction: column;
    }

    .provider-list-main b {
      overflow: hidden;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .provider-list-main small {
      color: var(--secondary);
      font-size: 9px;
    }

    .provider-list-row {
      min-height: 42px;
      gap: 10px;
      padding: 9px 12px;
      border-top-color: var(--separator);
    }

    .provider-list-row > .provider-icon {
      color: var(--text);
    }

    .provider-list-main b {
      overflow: hidden;
      font-size: 14px;
      font-weight: 600;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .provider-list-main small {
      font-size: 11px;
    }

    .switch input {
      position: absolute;
    }

    .switch span {
      width: 28px;
      height: 16px;
    }

    .chevron {
      font-size: 18px;
    }
  }
</style>
