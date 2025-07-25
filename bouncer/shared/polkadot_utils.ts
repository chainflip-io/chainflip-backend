import { BN } from '@polkadot/util';
import { AugmentedEvent, SubmittableExtrinsicFunction } from '@polkadot/api/types';
import { aliceKeyringPair } from 'shared/polkadot_keyring';
import { Event, polkadotSigningMutex, sleep } from 'shared/utils';
import { getPolkadotApi } from 'shared/utils/substrate';

export async function handleDispatchError(result: { dispatchError?: string }) {
  await using polkadot = await getPolkadotApi();
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

export async function submitAndGetEvent(
  call: ReturnType<SubmittableExtrinsicFunction<'promise'>>,
  eventMatch: AugmentedEvent<'promise'>,
): Promise<Event> {
  const alice = await aliceKeyringPair();
  let done = false;
  let event: Event = { name: '', data: [], block: 0, event_index: 0 };
  await polkadotSigningMutex.runExclusive(async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await call.signAndSend(alice, { nonce: -1 }, async (result: any) => {
      if (result.dispatchError) {
        done = true;
      }
      await handleDispatchError(result);
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
