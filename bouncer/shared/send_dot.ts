import { ApiPromise, WsProvider } from '@polkadot/api';
import Keyring from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { assetDecimals } from '@chainflip-io/cli';
import { polkadotSigningMutex, sleep, amountToFineAmount } from './utils';

export async function sendDot(address: string, amount: string) {
  const aliceUri = process.env.POLKADOT_ALICE_URI || '//Alice';
  const polkadotEndpoint = process.env.POLKADOT_ENDPOINT || 'ws://127.0.0.1:9945';

  const planckAmount = amountToFineAmount(amount, assetDecimals.DOT);
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const alice = keyring.createFromUri(aliceUri);
  const polkadot = await ApiPromise.create({
    provider: new WsProvider(polkadotEndpoint),
    noInitWarn: true,
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let resolve: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let reject: any;
  const promise = new Promise((resolve_, reject_) => {
    resolve = resolve_;
    reject = reject_;
  });

  // Ensure that both of these have been assigned from the callback above
  while (!resolve || !reject) {
    await sleep(1);
  }

  // The mutex ensures that we use the right nonces by eliminating certain
  // race conditions (this doesn't impact performance significantly as
  // waiting for block confirmation can still be done concurrently)
  await polkadotSigningMutex.runExclusive(async () => {
    await polkadot.tx.balances
      .transfer(address, parseInt(planckAmount))
      .signAndSend(alice, { nonce: -1 }, ({ status, dispatchError }) => {
        if (dispatchError !== undefined) {
          if (dispatchError.isModule) {
            const decoded = polkadot.registry.findMetaError(dispatchError.asModule);
            const { docs, name, section } = decoded;
            reject(new Error(`${section}.${name}: ${docs.join(' ')}`));
          } else {
            reject(new Error('Error: ' + dispatchError.toString()));
          }
        }
        if (status.isInBlock || status.isFinalized) {
          resolve();
        }
      });
  });

  return promise;
}
