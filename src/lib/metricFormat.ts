import { formattingLocale } from './i18n.svelte';

export type MetricNumberKind = 'percent' | 'dollars' | 'count';
export type MetricNumberStyle = 'tray' | 'row' | 'full';

interface LocaleFormatters {
  compact: Intl.NumberFormat;
  rowNumber: Intl.NumberFormat;
  fullNumber: Intl.NumberFormat;
  currency: Intl.NumberFormat;
  wholeDollar: Intl.NumberFormat;
}

const formatterCache = new Map<string, LocaleFormatters>();

function formatters(): LocaleFormatters {
  const locale = formattingLocale();
  let cached = formatterCache.get(locale);
  if (!cached) {
    cached = {
      compact: new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 }),
      rowNumber: new Intl.NumberFormat(locale, {
        minimumFractionDigits: 0,
        maximumFractionDigits: 1,
      }),
      fullNumber: new Intl.NumberFormat(locale, {
        minimumFractionDigits: 0,
        maximumFractionDigits: 1,
      }),
      currency: new Intl.NumberFormat(locale, {
        style: 'currency',
        currency: 'USD',
        minimumFractionDigits: 2,
        maximumFractionDigits: 2,
      }),
      wholeDollar: new Intl.NumberFormat(locale, {
        style: 'currency',
        currency: 'USD',
        minimumFractionDigits: 0,
        maximumFractionDigits: 0,
      }),
    };
    formatterCache.set(locale, cached);
  }
  return cached;
}

export function formatMetricNumber(
  value: number,
  kind: MetricNumberKind,
  style: MetricNumberStyle,
) {
  if (!Number.isFinite(value)) return '—';
  const formatter = formatters();
  if (kind === 'percent') return `${Math.round(Math.min(100, Math.max(0, value)))}%`;
  if (kind === 'dollars') {
    if (Math.abs(value) >= 1000 && style !== 'full') {
      return `$${formatter.compact.format(value)}`;
    }
    return style === 'tray'
      ? formatter.wholeDollar.format(value)
      : formatter.currency.format(value);
  }
  if (style !== 'full' && Math.abs(value) >= 1000) return formatter.compact.format(value);
  return (style === 'full' ? formatter.fullNumber : formatter.rowNumber).format(value);
}

export function formatMetricValue(
  value: number,
  kind: MetricNumberKind,
  style: MetricNumberStyle,
  label?: string,
) {
  const formatted = formatMetricNumber(value, kind, style);
  return label ? `${formatted} ${label}` : formatted;
}
