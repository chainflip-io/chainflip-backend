#!/usr/bin/env pnpm tsx

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { exec } from 'child_process';
import { sleep, runWithTimeout } from '../shared/utils';

let chainflip: ApiPromise;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function observeEvent(eventName: string): Promise<any> {
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

async function main(): Promise<void> {
  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI ?? '//LP_1';
  const lp = keyring.createFromUri(lpUri);
  chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });

  console.log('Requesting ETH deposit address');
  await chainflip.tx.liquidityProvider.requestLiquidityDepositAddress('Eth').signAndSend(lp);
  const ethIngressKey = (
    await observeEvent('liquidityProvider:LiquidityDepositAddressReady')
  )[1].toJSON().eth as string;
  console.log(`Found ETH address: ${ethIngressKey}`);

  exec(`./commands/fund_eth.ts ${ethIngressKey} 10`, { timeout: 20000 }, (err, stdout, stderr) => {
    if (stderr !== '') process.stdout.write(stderr);
    if (err !== null) {
      console.error(err);
      process.exit(1);
    }
    if (stdout !== '') process.stdout.write(stdout);
  });
  await observeEvent('liquidityProvider:AccountCredited');
  console.log('Successfully witnessed transfer!');
  process.exit(0);
}

// Allow roughly 100 seconds (7 blocks safety margin, each block ~12s apart)
// for the witnesser to detect the deposit before considering it a failure
runWithTimeout(main(), 100000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
