import { createStateChainKeypair } from './utils';

export async function newStatechainAddress(seed: string): Promise<string> {
  return createStateChainKeypair('//' + seed).address;
}
