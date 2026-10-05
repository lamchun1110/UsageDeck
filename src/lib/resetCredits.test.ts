import { describe, expect, it } from 'vitest';
import { availableResetCount, liveResetExpiries } from './resetCredits';
import type { ValueMetric } from './types';

describe('reset credits', () => {
  const now = Date.parse('2026-10-05T10:00:00Z');
  const metric: ValueMetric = {
    id: 'rateLimitResets',
    label: 'Rate Limit Resets',
    values: [{ number: 3, kind: 'count', estimated: false }],
    expiriesAt: ['2026-10-05T12:00:00Z', '2026-10-05T10:30:00+01:00', 'invalid'],
  };

  it('ages a cached count using known expiries and sorts by instant', () => {
    expect(availableResetCount(metric, now)).toBe(2);
    expect(liveResetExpiries(metric, now)).toEqual(['2026-10-05T12:00:00Z']);
    expect(availableResetCount(metric, now + 2 * 60 * 60 * 1000)).toBe(1);
    expect(liveResetExpiries(metric, now + 2 * 60 * 60 * 1000)).toEqual([]);
  });

  it('distinguishes unavailable data from a reported zero and caps inconsistent expiry lists', () => {
    expect(availableResetCount(null, now)).toBeNull();
    expect(availableResetCount({ ...metric, values: [] }, now)).toBeNull();
    expect(
      availableResetCount(
        { ...metric, values: [{ number: NaN, kind: 'count', estimated: false }] },
        now,
      ),
    ).toBeNull();
    const zero = { ...metric, values: [{ number: 0, kind: 'count' as const, estimated: false }] };
    expect(availableResetCount(zero, now)).toBe(0);
    expect(liveResetExpiries(zero, now)).toEqual([]);
  });
});
