<script lang="ts">
  import { t } from './i18n.svelte';
</script>

<span class="sr-only">{t('app.loading')}</span>
<div class="dashboard-skeleton" aria-hidden="true">
  {#each [0, 1] as card (card)}
    <section class="skeleton-card">
      <header>
        <i class="skeleton-bone skeleton-bone--grip"></i>
        <i class="skeleton-bone skeleton-bone--title"></i>
        <i class="skeleton-bone skeleton-bone--pill"></i>
      </header>
      {#each [0, 1] as row (row)}
        <div class="skeleton-row">
          <i class="skeleton-bone skeleton-bone--label"></i>
          <i class="skeleton-bone skeleton-bone--value"></i>
        </div>
        <i class="skeleton-bone skeleton-bone--meter"></i>
      {/each}
    </section>
  {/each}
</div>

<style>
  :global {
    .dashboard-skeleton {
      display: grid;
      gap: 10px;
      padding: 10px 14px;
    }

    .skeleton-card {
      display: grid;
      gap: 9px;
      padding: 11px 12px;
      border: 1px solid var(--surface-border);
      border-radius: 12px;
      background: var(--card);
    }

    .skeleton-card header {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .skeleton-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }

    .skeleton-bone {
      display: block;
      border-radius: 5px;
      background: var(--button-hover);
      animation: skeleton-pulse 1.4s ease-in-out infinite;
    }

    .skeleton-bone--grip {
      width: 13px;
      height: 13px;
    }

    .skeleton-bone--title {
      width: 64px;
      height: 13px;
    }

    .skeleton-bone--pill {
      width: 38px;
      height: 11px;
      margin-left: auto;
      border-radius: 999px;
    }

    .skeleton-bone--label {
      width: 56px;
      height: 11px;
    }

    .skeleton-bone--value {
      width: 46px;
      height: 11px;
    }

    .skeleton-bone--meter {
      width: 100%;
      height: 6px;
      border-radius: 999px;
    }

    @keyframes skeleton-pulse {
      0%,
      100% {
        opacity: 1;
      }
      50% {
        opacity: 0.45;
      }
    }
  }
</style>
