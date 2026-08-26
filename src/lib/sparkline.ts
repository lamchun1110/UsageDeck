/**
 * Maps a used-percent series onto a tiny sparkline polyline. Values are clamped to 0-100, the
 * first and last samples always land on the horizontal extremes, and a flat series still draws
 * a visible line centered by the caller's stroke width.
 */
export function sparklinePoints(values: number[], width = 56, height = 16, padding = 1): string {
  if (values.length < 2) return '';
  const usableHeight = height - padding * 2;
  const step = width / (values.length - 1);
  return values
    .map((value, index) => {
      const clamped = Math.min(100, Math.max(0, value));
      const x = (index * step).toFixed(1);
      const y = (padding + usableHeight * (1 - clamped / 100)).toFixed(1);
      return `${x},${y}`;
    })
    .join(' ');
}
