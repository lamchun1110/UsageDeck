import { describe, expect, it } from 'vitest';
import { formatMetricNumber, formatMetricValue } from './metricFormat';

describe('shared metric formatting', () => {
  it('keeps row values compact and tooltip values exact', () => {
    expect(formatMetricNumber(2059.07, 'dollars', 'row')).toBe('$2.1K');
    expect(formatMetricNumber(2059.07, 'dollars', 'full')).toBe('$2,059.07');
    expect(formatMetricValue(1_506_025_363, 'count', 'row', 'tokens')).toBe('1.5B tokens');
    expect(formatMetricValue(1_506_025_363, 'count', 'full', 'tokens')).toBe(
      '1,506,025,363 tokens',
    );
  });
});
