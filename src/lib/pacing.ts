import { formattingLocale, t } from './i18n.svelte';
import type { QuotaWindow } from './types';

export type PaceSeverity = 'level' | 'healthy' | 'close' | 'runningOut' | 'spent';

export interface PaceProjection {
  severity: PaceSeverity;
  projectedUsedPercent: number | null;
  evenPacePercent: number | null;
  runOutAt: number | null;
}

export function projectPace(window: QuotaWindow, now: number): PaceProjection {
  const used = clamp(window.usedPercent, 0, 100);
  if (isVisiblySpent(window, used)) {
    return { severity: 'spent', projectedUsedPercent: 100, evenPacePercent: null, runOutAt: now };
  }
  if (used <= 0) return level();
  const reset = window.resetsAt ? new Date(window.resetsAt).getTime() : Number.NaN;
  if (!Number.isFinite(reset) || reset <= now || window.periodSeconds <= 0) return level();
  const periodMs = window.periodSeconds * 1000;
  const start = reset - periodMs;
  const elapsed = Math.max(0, now - start);
  const progress = clamp(elapsed / periodMs, 0, 1);
  if (elapsed < Math.max(60_000, periodMs * 0.01)) return level();
  const projected = used / progress;
  if (projected <= 90) {
    return {
      severity: 'healthy',
      projectedUsedPercent: projected,
      evenPacePercent: progress * 100,
      runOutAt: null,
    };
  }
  if (used < 5) return level();
  if (projected <= 100) {
    const spare = Math.round(100 - projected);
    return {
      severity: spare >= 1 ? 'close' : 'runningOut',
      projectedUsedPercent: projected,
      evenPacePercent: progress * 100,
      runOutAt: null,
    };
  }
  const candidate = start + (elapsed * 100) / used;
  return {
    severity: 'runningOut',
    projectedUsedPercent: projected,
    evenPacePercent: progress * 100,
    runOutAt: candidate > now && candidate < reset ? candidate : null,
  };
}

export function isFreshSessionWindow(window: QuotaWindow, now: number, isSessionWindow: boolean) {
  if (!isSessionWindow || window.usedPercent > 0 || !window.resetsAt) return false;
  const reset = new Date(window.resetsAt).getTime();
  return Number.isFinite(reset) && now < reset;
}

export function paceTooltip(value: PaceProjection) {
  if (value.severity === 'level') return null;
  if (value.severity === 'spent') return t('quota.limitReached');
  const projected = value.projectedUsedPercent;
  if (projected === null) return null;
  if (value.severity === 'healthy')
    return t('pace.leftAtReset', { percent: Math.round(100 - projected) });
  if (value.severity === 'close') return t('pace.usedAtReset', { percent: Math.round(projected) });
  if (projected <= 100) return t('pace.fullAtReset');
  return t('pace.overAtReset', { percent: Math.max(1, Math.round(projected - 100)) });
}

type TimeFormat = 'system' | 'twelveHour' | 'twentyFourHour';

export function formatReset(
  value: string | null,
  now: number,
  mode: 'countdown' | 'exact',
  timeFormat: TimeFormat = 'system',
) {
  if (!value) return t('quota.resetUnavailable');
  const reset = new Date(value).getTime();
  if (!Number.isFinite(reset)) return t('quota.resetUnavailable');
  return formatDeadline('deadline.resets', reset, now, mode, timeFormat);
}

/** The reset deadline without its "Resets" prefix ("in 2h", "today at 3:04 PM", "soon"). */
export function formatResetDetail(
  value: string,
  now: number,
  mode: 'countdown' | 'exact',
  timeFormat: TimeFormat = 'system',
) {
  const reset = new Date(value).getTime();
  if (!Number.isFinite(reset)) return null;
  return renderDeadlineParts(deadlineParts(reset, now, mode, timeFormat));
}

/** Unprefixed reset deadline text plus a locale-independent kind for logic like imminent checks. */
export function formatResetParts(
  value: string,
  now: number,
  mode: 'countdown' | 'exact',
  timeFormat: TimeFormat = 'system',
): { kind: DeadlineKind; text: string } | null {
  const reset = new Date(value).getTime();
  if (!Number.isFinite(reset)) return null;
  const parts = deadlineParts(reset, now, mode, timeFormat);
  return { kind: parts.kind, text: renderDeadlineParts(parts) };
}

export function formatLimit(
  value: number | null,
  now: number,
  mode: 'countdown' | 'exact',
  timeFormat: TimeFormat = 'system',
) {
  if (value === null) return t('quota.limitReached');
  return formatDeadline('deadline.limit', value, now, mode, timeFormat);
}

function formatDeadline(
  prefixKey: string,
  value: number,
  now: number,
  mode: 'countdown' | 'exact',
  timeFormat: TimeFormat,
) {
  const prefix = t(prefixKey);
  const parts = deadlineParts(value, now, mode, timeFormat);
  switch (parts.kind) {
    case 'soon':
      return t('deadline.soon', { prefix });
    case 'countdown':
      return parts.duration === undefined
        ? t('deadline.soon', { prefix })
        : t('deadline.countdown', { prefix, duration: parts.duration });
    case 'today':
      return parts.time === undefined
        ? t('deadline.soon', { prefix })
        : t('deadline.today', { prefix, time: parts.time });
    case 'tomorrow':
      return parts.time === undefined
        ? t('deadline.soon', { prefix })
        : t('deadline.tomorrow', { prefix, time: parts.time });
    case 'date':
      return parts.time === undefined || parts.date === undefined
        ? t('deadline.soon', { prefix })
        : t('deadline.date', { prefix, date: parts.date, time: parts.time });
  }
}

export type DeadlineKind = 'soon' | 'countdown' | 'today' | 'tomorrow' | 'date';

interface DeadlineParts {
  kind: DeadlineKind;
  duration?: string;
  time?: string;
  date?: string;
}

function deadlineParts(
  value: number,
  now: number,
  mode: 'countdown' | 'exact',
  timeFormat: TimeFormat,
): DeadlineParts {
  const remaining = value - now;
  if (remaining <= 0 || (mode === 'countdown' && remaining <= 5 * 60_000)) {
    return { kind: 'soon' };
  }
  if (mode === 'countdown') return { kind: 'countdown', duration: formatDuration(remaining) };

  const date = new Date(value);
  const current = new Date(now);
  const currentDay = Date.UTC(current.getFullYear(), current.getMonth(), current.getDate());
  const targetDay = Date.UTC(date.getFullYear(), date.getMonth(), date.getDate());
  const dayDifference = Math.round((targetDay - currentDay) / 86_400_000);
  const time = date.toLocaleTimeString([formattingLocale()], {
    hour: 'numeric',
    minute: '2-digit',
    hour12: timeFormat === 'system' ? undefined : timeFormat === 'twelveHour',
  });
  if (dayDifference <= 0) return { kind: 'today', time };
  if (dayDifference === 1) return { kind: 'tomorrow', time };
  const monthDay = new Intl.DateTimeFormat(formattingLocale(), {
    month: 'short',
    day: 'numeric',
  }).format(date);
  return { kind: 'date', time, date: monthDay };
}

function renderDeadlineParts(parts: DeadlineParts) {
  switch (parts.kind) {
    case 'soon':
      return t('deadline.soonDetail');
    case 'countdown':
      return parts.duration === undefined
        ? t('deadline.soonDetail')
        : t('deadline.countdownDetail', { duration: parts.duration });
    case 'today':
      return parts.time === undefined
        ? t('deadline.soonDetail')
        : t('deadline.todayDetail', { time: parts.time });
    case 'tomorrow':
      return parts.time === undefined
        ? t('deadline.soonDetail')
        : t('deadline.tomorrowDetail', { time: parts.time });
    case 'date':
      return parts.time === undefined || parts.date === undefined
        ? t('deadline.soonDetail')
        : t('deadline.dateDetail', { date: parts.date, time: parts.time });
  }
}

function formatDuration(milliseconds: number) {
  const minutes = Math.max(1, Math.ceil(milliseconds / 60_000));
  const days = Math.floor(minutes / 1_440);
  const hours = Math.floor((minutes % 1_440) / 60);
  const remainder = minutes % 60;
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return remainder > 0 ? `${hours}h ${remainder}m` : `${hours}h`;
  return `${remainder}m`;
}

function level(): PaceProjection {
  return { severity: 'level', projectedUsedPercent: null, evenPacePercent: null, runOutAt: null };
}

function isVisiblySpent(window: QuotaWindow, usedPercent: number) {
  if (
    window.format === 'dollars' &&
    window.usedValue !== null &&
    window.limitValue !== null &&
    window.limitValue > 0
  ) {
    return Math.round((window.limitValue - window.usedValue) * 100) / 100 <= 0;
  }
  return Math.round(100 - usedPercent) <= 0;
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
