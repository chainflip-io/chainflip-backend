import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';

import { ApiPromise, WsProvider } from '@polkadot/api';

export const runWithTimeout = <T>(promise: Promise<T>, millis: number): Promise<T> =>
  Promise.race([
    promise,
    sleep(millis).then(() => {
      throw new Error(`Timed out after ${millis} ms.`);
    }),
  ]);

export const sha256 = (data: string): Buffer => crypto.createHash('sha256').update(data).digest();

export { sleep };

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function observeEvent(eventName: string, chainflip: ApiPromise): Promise<any> {
  let result;
  let waiting = true;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;
      if (event.section === eventName.split(':')[0] && event.method === eventName.split(':')[1]) {
        result = event.data;
        waiting = false;
        unsubscribe();
      }
    });
  });
  while (waiting) {
    await sleep(1000);
  }
  return result;
}