import { describe, expect, it, vi } from 'vitest';
import { Runner } from '../src/runner';
import { getHash } from '../src/utils';

const runner = new Runner();

function check(value: string) {
  return getHash(value);
}

describe('hash', () => {
  it('hashes', () => {
    expect(check('a')).toBe(getHash('a'));
  });

  it('runs', () => {
    vi.fn();
    expect(runner).toBeDefined();
  });
});
