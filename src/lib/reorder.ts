import type { MetricLayout, ProviderLayout } from './types';

/**
 * Pure list operations shared by the dashboard and both Customize screens.
 * Each returns the reordered array, or null when nothing should change, so
 * callers own the settings update (provider patch or customization change).
 */

/** Moves one enabled provider before another, keeping disabled providers parked at the end. */
export function reorderProviders(
  providers: ProviderLayout[],
  draggedId: string,
  targetId: string,
): ProviderLayout[] | null {
  if (draggedId === targetId) return null;
  const enabled = providers.filter((provider) => provider.enabled);
  const from = enabled.findIndex((provider) => provider.id === draggedId);
  const to = enabled.findIndex((provider) => provider.id === targetId);
  if (from < 0 || to < 0) return null;
  const [moved] = enabled.splice(from, 1);
  enabled.splice(to, 0, moved);
  return [...enabled, ...providers.filter((provider) => !provider.enabled)];
}

/** Moves one metric before another, retargeting its section to the target's. */
export function reorderMetric(
  metrics: MetricLayout[],
  draggedMetricId: string,
  targetMetricId: string,
  targetSection: MetricLayout['section'],
): MetricLayout[] | null {
  if (draggedMetricId === targetMetricId) return null;
  const next = [...metrics];
  const from = next.findIndex((metric) => metric.id === draggedMetricId);
  const to = next.findIndex((metric) => metric.id === targetMetricId);
  if (from < 0 || to < 0) return null;
  const [source] = next.splice(from, 1);
  next.splice(to, 0, { ...source, section: targetSection });
  return next;
}

/** Moves a metric to the end of a section (or the start of the always-visible one). */
export function moveMetricIntoSection(
  metrics: MetricLayout[],
  draggedMetricId: string,
  section: MetricLayout['section'],
): MetricLayout[] | null {
  const next = [...metrics];
  const from = next.findIndex((metric) => metric.id === draggedMetricId);
  if (from < 0) return null;
  const [source] = next.splice(from, 1);
  const lastInSection = next.reduce(
    (last, metric, index) => (metric.section === section ? index : last),
    -1,
  );
  const insertAt =
    lastInSection >= 0 ? lastInSection + 1 : section === 'alwaysVisible' ? 0 : next.length;
  next.splice(insertAt, 0, { ...source, section });
  return next;
}
