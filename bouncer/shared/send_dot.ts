import {
  polkadotSigningMutex,
  sleep,
  amountToFineAmount,
  getPolkadotApi,
  assetDecimals,
} from './utils';
import { aliceKeyringPair } from './polkadot_keyring';

export async function sendDot(address: string, amount: string) {
  const planckAmount = amountToFineAmount(amount, assetDecimals('Dot'));
  const alice = await aliceKeyringPair();
  const polkadot = await getPolkadotApi();

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
