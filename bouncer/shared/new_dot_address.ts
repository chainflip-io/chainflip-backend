import Keyring from 'polkadot/keyring';

export async function newDotAddress(seed: string): Promise<string> {
  const keyring = new Keyring({ type: 'sr25519' });
  const { addressRaw } = keyring.createFromUri('//' + seed);

  return keyring.encodeAddress(addressRaw, 0);
}
