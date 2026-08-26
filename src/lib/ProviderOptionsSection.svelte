<script lang="ts">
  import SelectMenu from './SelectMenu.svelte';
  import { providerOptions, selectedChoice, withProviderOption } from './providerOptions';
  import type { ProviderCatalogIndex } from './metrics';
  import type { AppSettings, ProviderLayout } from './types';

  interface Props {
    settings: AppSettings;
    provider: ProviderLayout;
    catalog: ProviderCatalogIndex;
    onChange: (settings: AppSettings) => void;
  }

  let { settings, provider, catalog, onChange }: Props = $props();

  const options = $derived(providerOptions(catalog.provider(provider.id)));

  function choose(optionId: string, choiceId: string) {
    const option = options.find((candidate) => candidate.id === optionId);
    if (!option) return;
    const changed = withProviderOption(settings, provider.id, option, choiceId);
    if (changed !== settings) onChange(changed);
  }
</script>

{#if options.length > 0}
  <section class="provider-options-section" aria-labelledby={`provider-options-${provider.id}`}>
    <h2 id={`provider-options-${provider.id}`}>Connection</h2>
    <div class="provider-options-card">
      {#each options as option (option.id)}
        {@const selected = selectedChoice(settings, provider.id, option)}
        <div class="setting-row">
          <span>
            <b>{option.label}</b>
            {#if option.description}<small>{option.description}</small>{/if}
          </span>
          <SelectMenu
            label={option.label}
            value={selected}
            options={option.choices.map((choice) => ({
              value: choice.id,
              label: choice.label,
            }))}
            onChange={(value) => choose(option.id, value)}
          />
        </div>
      {/each}
    </div>
  </section>
{/if}

<style>
  .provider-options-section {
    margin-bottom: 14px;
  }

  .provider-options-section h2 {
    margin: 0;
    padding: 0 8px 4px;
    color: var(--secondary);
    font-size: 11px;
    font-weight: 600;
  }

  .provider-options-card {
    overflow: hidden;
    border-radius: 12px;
    background: var(--card);
  }

  .setting-row {
    display: flex;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
  }

  .setting-row + .setting-row {
    border-top: 1px solid var(--divider);
  }

  .setting-row span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .setting-row b {
    font-size: 12px;
    font-weight: 600;
  }

  .setting-row small {
    color: var(--tertiary);
    font-size: 11px;
    line-height: 1.35;
  }

  :global(:root[data-density='compact']) .setting-row {
    padding-top: 8px;
    padding-bottom: 8px;
  }
</style>
