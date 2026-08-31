import type { ProviderCatalogIndex } from './metrics';
import { t } from './i18n.svelte';
import { formatMetricNumber, formatMetricValue } from './metricFormat';
import { formatLimit, formatReset, projectPace } from './pacing';
import {
  providerFamily,
  providerIconColor,
  providerIconPath,
  providerIconViewBox,
} from './providerIconPaths';
import type {
  AppSettings,
  DailyUsage,
  ProviderLayout,
  ProviderSnapshot,
  QuotaWindow,
  UsagePeriod,
} from './types';

export const SHARE_CARD_WIDTH = 360;
export const SHARE_CARD_SCALE = 4;

const OUTER_PADDING = 16;
const CONTENT_GAP = 12;
const CARD_GUTTER = 5;
const CARD_RADIUS = 12;
const ROW_HORIZONTAL_PADDING = 14;
const HEADER_HEIGHT = 22;

export type ShareRow =
  | {
      kind: 'quota';
      label: string;
      reading: string;
      trailing: string;
      fillPercent: number;
      severity: 'normal' | 'warning' | 'critical';
      paceLabel: string | null;
    }
  | { kind: 'text'; label: string; value: string; condensed: boolean }
  | { kind: 'trend'; label: string; daily: DailyUsage[] };

interface SharePalette {
  tray: string;
  surface: string;
  text: string;
  secondary: string;
  track: string;
  fill: string;
  warning: string;
  critical: string;
  provider: (id: string) => string;
}

interface ProviderShareCardOptions {
  providerId: string;
  providerNames?: Record<string, string>;
  plan: string | null;
  rows: ShareRow[];
}

export function buildProviderShareRows(
  catalog: ProviderCatalogIndex,
  snapshot: ProviderSnapshot,
  layout: ProviderLayout,
  settings: AppSettings,
  now: number,
) {
  const alwaysVisible = layout.metrics.filter(
    (metric) => metric.enabled && metric.section === 'alwaysVisible',
  );
  const visible = layout.expanded
    ? [
        ...alwaysVisible,
        ...layout.metrics.filter((metric) => metric.enabled && metric.section === 'onDemand'),
      ]
    : alwaysVisible;
  const rows: ShareRow[] = [];
  let previousTextSection: ProviderLayout['metrics'][number]['section'] | null = null;

  for (const notice of snapshot.notices) {
    rows.push({ kind: 'text', label: notice.title, value: notice.message, condensed: false });
  }

  for (const metric of visible) {
    const definition = catalog.metric(metric.id);
    if (!definition) continue;
    const source = definition.source;
    if (source.kind === 'quota' || source.kind === 'quotaOrValue') {
      const quota = snapshot.quotas.find((item) => item.id === source.sourceId);
      if (quota) {
        rows.push(quotaShareRow(quota, settings, now));
      } else if (source.kind === 'quotaOrValue') {
        const valueMetric = snapshot.valueMetrics.find((item) => item.id === source.sourceId);
        rows.push({
          kind: 'text',
          label: definition.label,
          value: valueMetric
            ? valueMetric.values
                .map((value) =>
                  formatMetricValue(value.number, value.kind, 'row', value.label ?? undefined),
                )
                .join(' · ')
            : t('quota.noData'),
          condensed: previousTextSection === metric.section,
        });
        previousTextSection = metric.section;
        continue;
      } else {
        rows.push({
          kind: 'quota',
          label: definition.label,
          reading: t('quota.noData'),
          trailing: t('quota.resetUnavailable'),
          fillPercent: 0,
          severity: 'normal',
          paceLabel: null,
        });
      }
      previousTextSection = null;
      continue;
    }
    if (source.kind === 'trend') {
      rows.push({ kind: 'trend', label: definition.label, daily: snapshot.usage.daily });
      previousTextSection = null;
      continue;
    }

    if (source.kind === 'status') {
      const statusMetric = snapshot.statusMetrics.find((item) => item.id === source.sourceId);
      rows.push({
        kind: 'text',
        label: definition.label,
        value: statusMetric?.text ?? t('quota.noData'),
        condensed: previousTextSection === metric.section,
      });
      previousTextSection = metric.section;
      continue;
    }

    if (source.kind === 'value') {
      const valueMetric = snapshot.valueMetrics.find((item) => item.id === source.sourceId);
      rows.push({
        kind: 'text',
        label: definition.label,
        value: valueMetric
          ? valueMetric.values
              .map((value) =>
                formatMetricValue(value.number, value.kind, 'row', value.label ?? undefined),
              )
              .join(' · ')
          : t('quota.noData'),
        condensed: previousTextSection === metric.section,
      });
      previousTextSection = metric.section;
      continue;
    }

    if (source.kind !== 'usage') continue;
    const period = usagePeriod(snapshot, source.period);
    rows.push({
      kind: 'text',
      label: definition.label,
      value: usageReading(period),
      condensed: previousTextSection === metric.section,
    });
    previousTextSection = metric.section;
  }
  return rows;
}

export function providerShareCardHeight(rows: ShareRow[]) {
  const rowContentHeight = rows.length
    ? rows.reduce((height, row) => height + shareRowHeight(row), 0)
    : 45;
  const cardHeight = CARD_GUTTER * 2 + rowContentHeight;
  return OUTER_PADDING + HEADER_HEIGHT + CONTENT_GAP + cardHeight + OUTER_PADDING;
}

export function renderProviderShareCard(
  catalog: ProviderCatalogIndex,
  options: ProviderShareCardOptions,
) {
  const height = providerShareCardHeight(options.rows);
  const { canvas, context } = createCanvas(SHARE_CARD_WIDTH, height);
  const palette = canvasPalette();
  fillBackground(context, palette, SHARE_CARD_WIDTH, height);

  drawProviderHeader(
    context,
    palette,
    catalog,
    options.providerId,
    options.providerNames,
    options.plan,
  );
  const cardTop = OUTER_PADDING + HEADER_HEIGHT + CONTENT_GAP;
  const cardHeight =
    CARD_GUTTER * 2 +
    (options.rows.length ? options.rows.reduce((sum, row) => sum + shareRowHeight(row), 0) : 45);
  drawRoundedRect(
    context,
    OUTER_PADDING,
    cardTop,
    SHARE_CARD_WIDTH - OUTER_PADDING * 2,
    cardHeight,
    CARD_RADIUS,
    palette.surface,
  );

  let rowTop = cardTop + CARD_GUTTER;
  if (options.rows.length === 0) {
    context.fillStyle = palette.secondary;
    context.font = '12px system-ui';
    context.textAlign = 'center';
    context.fillText(t('share.noMetrics'), SHARE_CARD_WIDTH / 2, rowTop + 27);
    context.textAlign = 'left';
  } else {
    for (const row of options.rows) {
      drawShareRow(context, palette, row, rowTop);
      rowTop += shareRowHeight(row);
    }
  }
  return canvas;
}

export async function copyShareCard(canvas: HTMLCanvasElement, fallback: string) {
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob(
      (value) => (value ? resolve(value) : reject(new Error('PNG unavailable'))),
      'image/png',
    ),
  );
  if (typeof ClipboardItem !== 'undefined' && navigator.clipboard.write) {
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
  } else {
    await navigator.clipboard.writeText(fallback);
  }
}

function quotaShareRow(quota: QuotaWindow, settings: AppSettings, now: number): ShareRow {
  const used = clamp(quota.usedPercent, 0, 100);
  const remaining = Math.max(0, 100 - used);
  let reading = `${(settings.usageDisplay === 'used' ? used : remaining).toFixed(0)}% ${
    settings.usageDisplay === 'used' ? t('quota.used') : t('quota.left')
  }`;
  let fillPercent = settings.usageDisplay === 'used' ? used : remaining;
  if (quota.format === 'count' && quota.usedValue !== null && quota.limitValue !== null) {
    const displayed =
      settings.usageDisplay === 'left'
        ? Math.max(0, quota.limitValue - quota.usedValue)
        : quota.usedValue;
    reading = `${formatMetricNumber(displayed, 'count', 'row')} ${quota.unit?.trim() || t('quota.requests')} ${
      settings.usageDisplay === 'used' ? t('quota.used') : t('quota.left')
    }`;
  }
  if (quota.format === 'dollars' && quota.usedValue !== null) {
    const displayed =
      settings.usageDisplay === 'left' && quota.limitValue !== null
        ? Math.max(0, quota.limitValue - quota.usedValue)
        : quota.usedValue;
    reading = `$${displayed.toFixed(2)} ${
      settings.usageDisplay === 'left' ? t('quota.left') : t('quota.spent')
    }`;
    if (quota.limitValue !== null && quota.limitValue > 0) {
      fillPercent = (displayed / quota.limitValue) * 100;
    }
  }

  const pace = projectPace(quota, now);
  const severity =
    pace.severity === 'spent' || pace.severity === 'runningOut'
      ? 'critical'
      : pace.severity === 'close'
        ? 'warning'
        : Math.round(used) >= 90
          ? 'critical'
          : Math.round(used) >= 80
            ? 'warning'
            : 'normal';
  const paceLabel =
    pace.severity === 'spent'
      ? t('quota.limitReached')
      : pace.severity === 'runningOut'
        ? formatLimit(pace.runOutAt, now, settings.resetDisplay, settings.timeFormat)
        : pace.severity === 'close' && pace.projectedUsedPercent !== null
          ? t('quota.spare', { percent: Math.max(1, Math.round(100 - pace.projectedUsedPercent)) })
          : pace.severity === 'healthy' &&
              settings.alwaysShowPacing &&
              pace.projectedUsedPercent !== null
            ? t('quota.leftAtReset', {
                percent: Math.max(0, Math.round(100 - pace.projectedUsedPercent)),
              })
            : null;

  return {
    kind: 'quota',
    label: quota.label,
    reading,
    trailing: formatReset(quota.resetsAt, now, settings.resetDisplay, settings.timeFormat),
    fillPercent: clamp(fillPercent, 0, 100),
    severity,
    paceLabel,
  };
}

function usagePeriod(snapshot: ProviderSnapshot, sourceId: string) {
  if (sourceId === 'today') return snapshot.usage.today;
  if (sourceId === 'yesterday') return snapshot.usage.yesterday;
  return snapshot.usage.last30Days;
}

function usageReading(period: UsagePeriod | null) {
  if (!period) return t('quota.noData');
  const tokens = formatMetricValue(period.tokens, 'count', 'row', t('usage.tokens'));
  if (period.estimatedCostUsd === null) return tokens;
  return `${formatMetricNumber(period.estimatedCostUsd, 'dollars', 'row')} · ${tokens}`;
}

function shareRowHeight(row: ShareRow) {
  if (row.kind === 'quota') return 64;
  if (row.kind === 'trend') return 37;
  return row.condensed ? 23 : 27;
}

function createCanvas(width: number, height: number) {
  const canvas = document.createElement('canvas');
  canvas.width = width * SHARE_CARD_SCALE;
  canvas.height = Math.ceil(height * SHARE_CARD_SCALE);
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Canvas unavailable');
  context.scale(SHARE_CARD_SCALE, SHARE_CARD_SCALE);
  context.textBaseline = 'alphabetic';
  return { canvas, context };
}

function canvasPalette(): SharePalette {
  const styles = getComputedStyle(document.documentElement);
  const value = (name: string) => styles.getPropertyValue(name).trim();
  return {
    tray: value('--tray'),
    surface: value('--card'),
    text: value('--text'),
    secondary: value('--secondary'),
    track: value('--meter-track'),
    fill: value('--meter-fill'),
    warning: value('--meter-warning'),
    critical: value('--meter-critical'),
    provider: (id: string) => value(`--provider-${providerFamily(id)}`) || value('--provider'),
  };
}

function fillBackground(
  context: CanvasRenderingContext2D,
  palette: SharePalette,
  width: number,
  height: number,
) {
  context.fillStyle = palette.tray;
  context.fillRect(0, 0, width, height);
}

function drawProviderHeader(
  context: CanvasRenderingContext2D,
  palette: SharePalette,
  catalog: ProviderCatalogIndex,
  providerId: string,
  providerNames: Record<string, string> | undefined,
  plan: string | null,
) {
  const iconColor = providerIconColor(providerId) ?? palette.text;
  drawProviderMark(context, providerId, OUTER_PADDING, OUTER_PADDING, 22, iconColor);
  const name = catalog.displayName(providerId, providerNames);
  context.fillStyle = palette.text;
  context.font = '600 15px system-ui';
  // Long custom names must not push the plan label off the card: fit the name
  // to the full row first, then place the plan after whatever width remains.
  const nameMaxWidth = SHARE_CARD_WIDTH - OUTER_PADDING - 48;
  fitText(context, name, 48, 31, nameMaxWidth);
  if (!plan) return;
  const nameWidth = context.measureText(ellipsize(context, name, nameMaxWidth)).width;
  context.fillStyle = palette.secondary;
  context.font = '12px system-ui';
  const planX = 48 + nameWidth + 6;
  fitText(context, plan, planX, 31, Math.max(0, SHARE_CARD_WIDTH - OUTER_PADDING - planX));
}

function drawShareRow(
  context: CanvasRenderingContext2D,
  palette: SharePalette,
  row: ShareRow,
  top: number,
) {
  const left = OUTER_PADDING + ROW_HORIZONTAL_PADDING;
  const right = SHARE_CARD_WIDTH - OUTER_PADDING - ROW_HORIZONTAL_PADDING;
  if (row.kind === 'quota') {
    context.fillStyle = palette.text;
    context.font = '600 13px system-ui';
    fitText(context, row.label, left, top + 23, row.paceLabel ? 150 : right - left);
    if (row.paceLabel) {
      context.fillStyle = palette.secondary;
      context.font = '12px system-ui';
      context.textAlign = 'right';
      context.fillText(row.paceLabel, right, top + 23);
      context.textAlign = 'left';
    }
    drawRoundedRect(context, left, top + 31, right - left, 5, 3, palette.track);
    const fillWidth = Math.max(
      row.fillPercent > 0 ? 5 : 0,
      ((right - left) * row.fillPercent) / 100,
    );
    if (fillWidth > 0) {
      const fill =
        row.severity === 'critical'
          ? palette.critical
          : row.severity === 'warning'
            ? palette.warning
            : palette.fill;
      drawRoundedRect(context, left, top + 31, fillWidth, 5, Math.min(3, fillWidth / 2), fill);
    }
    context.fillStyle = palette.text;
    context.font = '500 12px system-ui';
    fitText(context, row.reading, left, top + 52, 130);
    context.fillStyle = palette.secondary;
    context.font = '12px system-ui';
    context.textAlign = 'right';
    fitTextRight(context, row.trailing, right, top + 52, 155);
    context.textAlign = 'left';
    return;
  }
  if (row.kind === 'trend') {
    context.fillStyle = palette.text;
    context.font = '600 12px system-ui';
    context.fillText(row.label, left, top + 23);
    drawTrend(context, palette, row.daily, right - 150, top + 9, 150, 18);
    return;
  }
  const baseline = top + (row.condensed ? 15 : 17);
  context.fillStyle = palette.text;
  context.font = '600 12px system-ui';
  fitText(context, row.label, left, baseline, 112);
  context.fillStyle = palette.text;
  context.font = '12px system-ui';
  context.textAlign = 'right';
  fitTextRight(context, row.value, right, baseline, 178);
  context.textAlign = 'left';
}

function drawTrend(
  context: CanvasRenderingContext2D,
  palette: SharePalette,
  daily: DailyUsage[],
  x: number,
  y: number,
  width: number,
  height: number,
) {
  const points = daily.slice(-30);
  const values = points.length ? points.map((point) => point.tokens) : [0];
  const max = Math.max(1, ...values);
  const gap = 1.5;
  const barWidth = Math.max(1, (width - gap * (values.length - 1)) / values.length);
  values.forEach((value, index) => {
    const barHeight = Math.max(value > 0 ? 3 : 2, (value / max) * height);
    drawRoundedRect(
      context,
      x + index * (barWidth + gap),
      y + height - barHeight,
      barWidth,
      barHeight,
      Math.min(1, barWidth / 2),
      palette.fill,
    );
  });
}

function drawProviderMark(
  context: CanvasRenderingContext2D,
  providerId: string,
  x: number,
  y: number,
  size: number,
  color: string,
) {
  const path = providerIconPath(providerId);
  if (!path || typeof Path2D === 'undefined') return;
  const placement = providerIconPlacement(providerId, x, y, size);
  context.save();
  context.translate(placement.x, placement.y);
  context.scale(placement.scale, placement.scale);
  context.fillStyle = color;
  context.fill(new Path2D(path));
  context.restore();
}

export function providerIconPlacement(providerId: string, x: number, y: number, size: number) {
  const values = providerIconViewBox(providerId)
    .trim()
    .split(/[\s,]+/)
    .map(Number);
  const [minX, minY, width, height] = values;
  if (
    values.length !== 4 ||
    !values.every(Number.isFinite) ||
    width <= 0 ||
    height <= 0 ||
    !Number.isFinite(size) ||
    size <= 0
  ) {
    return { x, y, scale: size / 100 };
  }

  const scale = Math.min(size / width, size / height);
  return {
    x: x + (size - width * scale) / 2 - minX * scale,
    y: y + (size - height * scale) / 2 - minY * scale,
    scale,
  };
}

function drawRoundedRect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
  fill: string,
) {
  context.fillStyle = fill;
  context.beginPath();
  context.roundRect(x, y, width, height, radius);
  context.fill();
}

function fitText(
  context: CanvasRenderingContext2D,
  value: string,
  x: number,
  y: number,
  maxWidth: number,
) {
  context.fillText(ellipsize(context, value, maxWidth), x, y);
}

function fitTextRight(
  context: CanvasRenderingContext2D,
  value: string,
  right: number,
  y: number,
  maxWidth: number,
) {
  context.fillText(ellipsize(context, value, maxWidth), right, y);
}

function ellipsize(context: CanvasRenderingContext2D, value: string, maxWidth: number) {
  if (context.measureText(value).width <= maxWidth) return value;
  let result = value;
  while (result.length > 1 && context.measureText(`${result}…`).width > maxWidth) {
    result = result.slice(0, -1);
  }
  return `${result}…`;
}

function clamp(value: number, minimum: number, maximum: number) {
  // Math.min/Math.max pass NaN through; share cards must render a flat gauge
  // for a non-finite percent, never "NaN%".
  if (!Number.isFinite(value)) return minimum;
  return Math.min(maximum, Math.max(minimum, value));
}
