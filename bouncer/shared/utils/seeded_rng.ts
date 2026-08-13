/**
 * A small, fast, self-contained seedable PRNG (mulberry32) returning values in [0, 1).
 *
 * Given the same seed it always yields the same sequence, so callers that need reproducible
 * "randomness" — e.g. sampling a deterministic set of test cases — can rely on it. Distribution
 * quality is good enough for shuffling/sampling; it is not cryptographically secure.
 */
/* eslint-disable no-bitwise, operator-assignment */
export function seededRng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
/* eslint-enable no-bitwise, operator-assignment */
