import { BN } from '@polkadot/util';
import { aliceKeyringPair } from '../shared/polkadot_keyring';
import { Event, getPolkadotApi, polkadotSigningMutex, sleep } from '../shared/utils';

// TODO: Move getPolkadotApi and other stuff from utils to here
const polkadot = await getPolkadotApi();

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function handleDispatchError(result: any) {
  if (result.dispatchError) {
    const dispatchError = JSON.parse(result.dispatchError);
    if (dispatchError.module) {
      const errorIndex = {
        index: new BN(dispatchError.module.index, 'hex'),
        error: new Uint8Array(Buffer.from(dispatchError.module.error.slice(2), 'hex')),
      };
      const { docs, name, section } = polkadot.registry.findMetaError(errorIndex);
      throw new Error('dispatchError:' + section + '.' + name + ': ' + docs);
    } else {
      throw new Error('dispatchError: ' + JSON.stringify(dispatchError));
    }
  }
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function submitAndGetEvent(call: any, eventMatch: any): Promise<Event> {
  const alice = await aliceKeyringPair();
  let done = false;
  let event: Event = { name: '', data: [], block: 0, event_index: 0 };
  await polkadotSigningMutex.runExclusive(async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await call.signAndSend(alice, { nonce: -1 }, (result: any) => {
      if (result.dispatchError) {
        done = true;
      }
      handleDispatchError(result);
      if (result.isInBlock) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        result.events.forEach((eventRecord: any) => {
          if (eventMatch.is(eventRecord.event)) {
            event = eventRecord.event;
            done = true;
          }
        });
        if (!done) {
          done = true;
          throw new Error('Event was not found in block: ' + JSON.stringify(eventMatch));
        }
      }
    });
  });
  while (!done) {
    await sleep(1000);
  }
  return event;
}
