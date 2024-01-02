import Keyring from '@polkadot/keyring';
import { KeyringPair } from '@polkadot/keyring/types';
import { cryptoWaitReady } from '@polkadot/util-crypto';

const aliceUri = process.env.POLKADOT_ALICE_URI || '//Alice';

let alice: KeyringPair | undefined;

/// Get the Alice keyring pair.
/// Using a singleton instance of the keyring pair to avoid nonce issues.
export async function aliceKeyringPair() {
  if (!alice) {
    await cryptoWaitReady();
    const keyring = new Keyring({ type: 'sr25519' });
    alice = keyring.createFromUri(aliceUri);
  }
  return alice;
}
