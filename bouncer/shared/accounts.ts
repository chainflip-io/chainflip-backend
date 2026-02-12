import { Mutex } from 'async-mutex';
import { KeyedMutex } from 'shared/utils/keyed_mutex';

export const cfMutex = new KeyedMutex();
export const ethNonceMutex = new Mutex();
export const arbNonceMutex = new Mutex();
export const btcClientMutex = new Mutex();
