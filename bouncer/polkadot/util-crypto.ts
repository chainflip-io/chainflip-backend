/* eslint-disable no-restricted-imports */
import { cryptoWaitReady } from '@polkadot/util-crypto';

await cryptoWaitReady();

export * from '@polkadot/util-crypto';
