import Keyring from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';

export async function newFlipAddress(seed: string): Promise<string> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const { address } = keyring.createFromUri('//' + seed);

  return address;
}
