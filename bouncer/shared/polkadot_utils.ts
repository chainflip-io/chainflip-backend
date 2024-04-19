import { BN } from '@polkadot/util';
import { aliceKeyringPair } from '../shared/polkadot_keyring';
import { Event, deferredPromise, getPolkadotApi, polkadotSigningMutex } from '../shared/utils';

// TODO: Move getPolkadotApi and other stuff from utils to here

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function handleDispatchError(result: any) {
  await using polkadot = await getPolkadotApi();
  if (result.dispatchError) {
    const dispatchError = JSON.parse(result.dispatchError);
    if (dispatchError.module) {
      const errorIndex = {
        index: new BN(dispatchError.module.index, 'hex'),
        error: new Uint8Array(Buffer.from(dispatchError.module.error.slice(2), 'hex')),
      };
      const { docs, name, section } = polkadot.registry.findMetaError(errorIndex);
      return new Error('dispatchError:' + section + '.' + name + ': ' + docs);
    }

    return new Error('dispatchError: ' + JSON.stringify(dispatchError));
  }

  return null;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function submitAndGetEvent(call: any, eventMatch: any): Promise<Event> {
  const alice = await aliceKeyringPair();

  const { promise, resolve, reject } = deferredPromise<Event>();

  await polkadotSigningMutex.runExclusive(async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await call.signAndSend(alice, { nonce: -1 }, async (result: any) => {
      const error = await handleDispatchError(result);

      if (error) {
        reject(error);
      } else if (result.isInBlock) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        result.events.forEach((eventRecord: any) => {
          if (eventMatch.is(eventRecord.event)) {
            resolve(eventRecord.event);
          }
        });

        reject(new Error('Event was not found in block: ' + JSON.stringify(eventMatch)));
      }
    });
  });

  return promise;
}
