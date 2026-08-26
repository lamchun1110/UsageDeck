import { describe, expect, it } from 'vitest';

import { sparklinePoints } from './sparkline';

describe('sparklinePoints', () => {
  it('returns nothing for fewer than two samples', () => {
    expect(sparklinePoints([])).toBe('');
    expect(sparklinePoints([42])).toBe('');
  });

  it('pins the first and last samples to the horizontal extremes', () => {
    const points = sparklinePoints([0, 100], 56, 16).split(' ');
    expect(points[0].split(',')[0]).toBe('0.0');
    expect(points[1].split(',')[0]).toBe('56.0');
  });

  it('maps full usage to the top edge and empty usage to the bottom edge', () => {
    const [bottom, top] = sparklinePoints([0, 100], 56, 16, 1).split(' ');
    expect(Number(bottom.split(',')[1])).toBe(15);
    expect(Number(top.split(',')[1])).toBe(1);
  });

  it('clamps out-of-range samples into the box', () => {
    const [low, high] = sparklinePoints([-30, 140], 56, 16, 1).split(' ');
    expect(Number(low.split(',')[1])).toBe(15);
    expect(Number(high.split(',')[1])).toBe(1);
  });
});
