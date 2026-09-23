import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import ModelUsageDetail from './ModelUsageDetail.svelte';
import type { ModelUsageBreakdown, ModelUsageEntry } from './types';

function entry(model: string, totalTokens: number, costUsd: number | null): ModelUsageEntry {
  return { model, totalTokens, costUsd };
}

function breakdown(models: ModelUsageEntry[]): ModelUsageBreakdown {
  return { models, sourceNote: 'From your Codex logs (estimated)' };
}

function renderDetail(models: ModelUsageEntry[]) {
  render(ModelUsageDetail, {
    props: {
      title: 'Today · model usage',
      breakdown: breakdown(models),
      top: 10,
      onEnter: vi.fn(),
      onLeave: vi.fn(),
    },
  });
}

describe('ModelUsageDetail share selection and rounding', () => {
  afterEach(cleanup);

  it('splits by cost with largest-remainder rounding when every model is priced', () => {
    renderDetail([entry('model-a', 100, 1), entry('model-b', 100, 1), entry('model-c', 100, 1)]);
    // Three equal shares floor to 33% each; the leftover point goes to the
    // first row by the index tie-break, so the row percentages sum to 100.
    expect(screen.getByText('34%')).toBeInTheDocument();
    expect(screen.getAllByText('33%')).toHaveLength(2);
  });

  it('falls back to token shares when any model is unpriced', () => {
    renderDetail([entry('model-a', 300, 9.5), entry('model-b', 100, null)]);
    // Cost shares are impossible (model-b unpriced), so 300:100 tokens win.
    expect(screen.getByText('75%')).toBeInTheDocument();
    expect(screen.getByText('25%')).toBeInTheDocument();
    expect(screen.getByText('—')).toBeInTheDocument();
  });

  it('renders zero shares instead of NaN when there is no usage', () => {
    renderDetail([entry('model-a', 0, 0), entry('model-b', 0, null)]);
    expect(screen.getAllByText('0%')).toHaveLength(2);
  });
});
