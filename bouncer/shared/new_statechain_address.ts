import Keyring from '../polkadot/keyring';

export async function newStatechainAddress(seed: string): Promise<string> {
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const { address } = keyring.createFromUri('//' + seed);

  return address;
}
