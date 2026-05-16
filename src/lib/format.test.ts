import { describe, it, expect } from 'vitest';
import { formatBytes, formatRate, formatEta } from './format';

describe('formatters', () => {
  it('formats bytes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1024 * 1024 * 5.5)).toBe('5.5 MB');
  });

  it('formats rate', () => {
    expect(formatRate(2048)).toBe('2.0 KB/s');
  });

  it('formats eta', () => {
    expect(formatEta(null)).toBe('--');
    expect(formatEta(65)).toBe('1:05');
    expect(formatEta(3661)).toBe('1:01:01');
  });
});
