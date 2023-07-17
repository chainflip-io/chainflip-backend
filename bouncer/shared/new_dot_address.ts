import Keyring from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';

export async function newDotAddress(seed: string): Promise<string> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const { address } = keyring.createFromUri('//' + seed);

  return address;
}
