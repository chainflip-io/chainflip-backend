import { Keypair } from '@solana/web3.js';
import { sha256 } from 'shared/utils';

export function newSolAddress(seed: string): string {
  return Keypair.fromSeed(sha256(seed)).publicKey.toBase58();
}

// A Solana keypair derived deterministically from `rng`, so generated account addresses are
// reproducible from the seed rather than cryptographically random.
export function seededSolanaKeypair(rng: () => number): Keypair {
  const seed = new Uint8Array(32);
  for (let i = 0; i < 32; i++) {
    seed[i] = Math.floor(rng() * 256);
  }
  return Keypair.fromSeed(seed);
}
