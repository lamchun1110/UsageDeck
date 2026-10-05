import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ValueMetric from './ValueMetric.svelte';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

describe('ValueMetric', () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });
  afterEach(cleanup);
  it('renders combined credit values with an exact tooltip for large balances', () => {
    render(ValueMetric, {
      label: 'Extra Usage',
      metric: {
        id: 'credits',
        label: 'Extra Usage',
        values: [
          { number: 1200, kind: 'dollars', estimated: false },
          { number: 30000, kind: 'count', label: 'credits', estimated: false },
        ],
        expiriesAt: [],
      },
      now: Date.parse('2026-02-20T16:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });

    expect(screen.getByText('$1.2K · 30K credits')).toHaveAttribute(
      'data-tooltip',
      '$1,200.00 · 30,000 credits',
    );
  });

  it('marks only value rows that contain an estimated value', () => {
    render(ValueMetric, {
      label: 'Extra Usage',
      metric: {
        id: 'credits',
        label: 'Extra Usage',
        values: [
          { number: 4, kind: 'dollars', estimated: true },
          { number: 100, kind: 'count', label: 'credits', estimated: false },
        ],
        expiriesAt: [],
      },
      now: Date.parse('2026-02-20T16:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });

    expect(screen.getByLabelText('Estimated value')).toHaveAttribute(
      'data-tooltip',
      'Estimated locally, so it may differ from billed usage.',
    );
  });

  it('opens a sorted reset-expiry timeline and distinguishes count-only fallback', async () => {
    const { rerender } = render(ValueMetric, {
      label: 'Rate Limit Resets',
      providerId: 'codex',
      metric: {
        id: 'rateLimitResets',
        label: 'Rate Limit Resets',
        values: [{ number: 2, kind: 'count', label: 'available', estimated: false }],
        expiriesAt: ['2026-02-20T19:00:00Z', '2026-02-20T17:30:00Z'],
      },
      now: Date.parse('2026-02-20T16:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });

    const trigger = screen.getByRole('button', { name: 'Rate Limit Resets: 2 available' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await fireEvent.click(trigger);
    expect(screen.getByRole('dialog', { name: 'Rate Limit Resets details' })).toBeVisible();
    expect(screen.getByText('in 1h 30m')).toBeInTheDocument();
    expect(screen.getByText('in 3h')).toBeInTheDocument();

    rerender({
      label: 'Rate Limit Resets',
      providerId: 'codex',
      metric: {
        id: 'rateLimitResets',
        label: 'Rate Limit Resets',
        values: [{ number: 3, kind: 'count', label: 'available', estimated: false }],
        expiriesAt: [],
      },
      now: Date.parse('2026-02-20T16:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });
    expect(screen.getAllByText('3 available')).toHaveLength(2);
    expect(screen.getAllByText('Expiry times unavailable')).toHaveLength(2);
  });

  it('requires confirmation and claims one explicitly selected reset credit', async () => {
    mocks.invoke.mockResolvedValue('success');
    render(ValueMetric, {
      label: 'Rate Limit Resets',
      providerId: 'codex',
      metric: {
        id: 'rateLimitResets',
        label: 'Rate Limit Resets',
        values: [{ number: 1, kind: 'count', label: 'available', estimated: false }],
        expiriesAt: ['2026-02-20T19:00:00Z'],
      },
      now: Date.parse('2026-02-20T16:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Rate Limit Resets: 1 available' }));
    await fireEvent.click(screen.getByRole('button', { name: /Use reset expiring/ }));
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(screen.getByRole('group', { name: 'Use this reset?' })).toHaveAccessibleDescription(
      "Immediately reset your usage limits. This can't be undone.",
    );
    expect(
      screen.getByText("Immediately reset your usage limits. This can't be undone."),
    ).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus());
    await fireEvent.click(screen.getByRole('button', { name: 'Use reset' }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('claim_codex_reset_credit', {
        expiresAt: '2026-02-20T19:00:00Z',
        redeemRequestId: expect.any(String),
      }),
    );
    expect(await screen.findByText('Reset applied.')).toBeInTheDocument();
  });

  it('keeps reset confirmation open when focus moves into the detail panel', async () => {
    vi.useFakeTimers();
    try {
      render(ValueMetric, {
        label: 'Rate Limit Resets',
        providerId: 'codex',
        metric: {
          id: 'rateLimitResets',
          label: 'Rate Limit Resets',
          values: [{ number: 1, kind: 'count', label: 'available', estimated: false }],
          expiriesAt: ['2026-02-20T19:00:00Z'],
        },
        now: Date.parse('2026-02-20T16:00:00Z'),
        resetDisplay: 'countdown',
        timeFormat: 'twentyFourHour',
      });

      const trigger = screen.getByRole('button', { name: 'Rate Limit Resets: 1 available' });
      trigger.focus();
      await fireEvent.click(trigger);

      const use = screen.getByRole('button', { name: /Use reset expiring/ });
      use.focus();
      await fireEvent.click(use);
      await vi.advanceTimersByTimeAsync(181);

      expect(screen.getByText('Use this reset?')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
      expect(screen.getByRole('dialog', { name: 'Rate Limit Resets details' })).toBeVisible();
      expect(mocks.invoke).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('behaves as an anchored popover and closes with Escape', async () => {
    vi.useFakeTimers();
    try {
      render(ValueMetric, {
        label: 'Rate Limit Resets',
        providerId: 'codex',
        metric: {
          id: 'rateLimitResets',
          label: 'Rate Limit Resets',
          values: [{ number: 1, kind: 'count', label: 'available', estimated: false }],
          expiriesAt: ['2026-02-20T19:00:00Z'],
        },
        now: Date.parse('2026-02-20T16:00:00Z'),
        resetDisplay: 'countdown',
        timeFormat: 'twentyFourHour',
      });

      await fireEvent.click(screen.getByRole('button', { name: 'Rate Limit Resets: 1 available' }));
      expect(screen.queryByLabelText('Drag Rate Limit Resets panel')).not.toBeInTheDocument();
      expect(
        screen.queryByRole('button', { name: 'Close Rate Limit Resets' }),
      ).not.toBeInTheDocument();

      const use = screen.getByRole('button', { name: /Use reset expiring/ });
      await fireEvent.click(use);
      let cancel = screen.getByRole('button', { name: 'Cancel' });
      await vi.waitFor(() => expect(cancel).toHaveFocus());
      await fireEvent.click(cancel);
      expect(screen.queryByText('Use this reset?')).not.toBeInTheDocument();
      let restoredUse = screen.getByRole('button', { name: /Use reset expiring/ });
      await vi.waitFor(() => expect(restoredUse).toHaveFocus());

      await fireEvent.click(restoredUse);
      cancel = screen.getByRole('button', { name: 'Cancel' });
      await vi.waitFor(() => expect(cancel).toHaveFocus());
      await fireEvent.keyDown(cancel, { key: 'Escape' });
      expect(screen.queryByText('Use this reset?')).not.toBeInTheDocument();
      const dialog = screen.getByRole('dialog', { name: 'Rate Limit Resets details' });
      expect(dialog).toBeVisible();
      restoredUse = screen.getByRole('button', { name: /Use reset expiring/ });
      await vi.waitFor(() => expect(restoredUse).toHaveFocus());
      await fireEvent.mouseLeave(dialog);
      await vi.advanceTimersByTimeAsync(181);
      expect(dialog).toBeVisible();

      await fireEvent.keyDown(restoredUse, { key: 'Escape' });
      expect(
        screen.queryByRole('dialog', { name: 'Rate Limit Resets details' }),
      ).not.toBeInTheDocument();
      await vi.waitFor(() =>
        expect(
          screen.getByRole('button', { name: 'Rate Limit Resets: 1 available' }),
        ).toHaveFocus(),
      );
      await vi.advanceTimersByTimeAsync(351);
      expect(
        screen.queryByRole('dialog', { name: 'Rate Limit Resets details' }),
      ).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
  it('marks an unknown provider count unavailable and keeps the detail read-only', async () => {
    render(ValueMetric, {
      label: 'Rate Limit Resets',
      providerId: 'claude',
      resetMetric: true,
      metric: null,
      now: Date.parse('2026-10-05T10:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Rate Limit Resets: Unavailable' }));
    expect(screen.getByText(/Claude did not return reset-offer data/)).toBeInTheDocument();
    expect(screen.queryByText('No rate limit resets available')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Use reset/ })).not.toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it('removes an expired credit from the count, timeline, and redemption controls as time advances', async () => {
    const props = {
      label: 'Rate Limit Resets',
      providerId: 'codex',
      metric: {
        id: 'rateLimitResets',
        label: 'Rate Limit Resets',
        values: [{ number: 1, kind: 'count' as const, estimated: false }],
        expiriesAt: ['2026-10-05T11:00:00Z'],
      },
      now: Date.parse('2026-10-05T10:00:00Z'),
      resetDisplay: 'countdown' as const,
      timeFormat: 'twentyFourHour' as const,
    };
    const { rerender } = render(ValueMetric, props);
    await fireEvent.click(screen.getByRole('button', { name: 'Rate Limit Resets: 1 available' }));
    expect(screen.getByRole('button', { name: /Use reset expiring/ })).toBeInTheDocument();
    await rerender({ ...props, now: Date.parse('2026-10-05T11:00:00Z') });
    expect(
      screen.getByRole('button', { name: 'Rate Limit Resets: 0 available' }),
    ).toBeInTheDocument();
    expect(screen.getByText('No rate limit resets available')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Use reset expiring/ })).not.toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it('explains a ZCode key replacement and imports it only after the selected account action', async () => {
    mocks.invoke.mockResolvedValue({
      state: { providerId: 'zai@personal', status: 'saved' },
      warning: null,
    });
    render(ValueMetric, {
      label: 'Rate Limit Resets',
      providerId: 'zai@personal',
      resetMetric: true,
      metric: null,
      now: Date.parse('2026-10-05T00:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Rate Limit Resets: Unavailable' }));
    expect(screen.getByText(/replaces this card’s saved key/)).toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Use ZCode API key' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('use_zcode_api_key', {
        providerId: 'zai@personal',
      }),
    );
    expect(screen.queryByRole('button', { name: /Use reset expiring/ })).not.toBeInTheDocument();
  });

  it('never redeems a different Codex account through the default account command', async () => {
    render(ValueMetric, {
      label: 'Rate Limit Resets',
      providerId: 'codex@1234abcd',
      metric: {
        id: 'rateLimitResets',
        label: 'Rate Limit Resets',
        values: [{ number: 2, kind: 'count', estimated: false }],
        expiriesAt: ['2026-10-05T11:00:00Z'],
      },
      now: Date.parse('2026-10-05T10:00:00Z'),
      resetDisplay: 'countdown',
      timeFormat: 'twentyFourHour',
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Rate Limit Resets: 2 available' }));
    expect(screen.queryByRole('button', { name: /Use reset expiring/ })).not.toBeInTheDocument();
    expect(screen.getByText('1 more with unknown expiry times')).toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
