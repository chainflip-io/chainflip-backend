import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';

export const runWithTimeout = async <T>(promise: Promise<T>, millis: number): Promise<T> =>
  await Promise.race([
    promise,
    sleep(millis).then(() => {
      throw new Error(`Timed out after ${millis} ms.`);
    }),
  ]);

export const sha256 = (data: string): Buffer => crypto.createHash('sha256').update(data).digest();

export { sleep };
