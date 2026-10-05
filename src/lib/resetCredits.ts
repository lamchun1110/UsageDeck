import type { ValueMetric } from './types';

export function availableResetCount(metric: ValueMetric | null, now: number): number | null {
  const count = metric?.values[0]?.number;
  if (count === undefined || !Number.isFinite(count) || count < 0) return null;
  const expired = metric?.expiriesAt.filter((expiry) => Date.parse(expiry) <= now).length ?? 0;
  return Math.max(0, Math.floor(count) - expired);
}

export function liveResetExpiries(metric: ValueMetric | null, now: number): string[] {
  const count = availableResetCount(metric, now) ?? 0;
  return (metric?.expiriesAt ?? [])
    .filter((expiry) => Date.parse(expiry) > now)
    .sort((a, b) => Date.parse(a) - Date.parse(b))
    .slice(0, count);
}
