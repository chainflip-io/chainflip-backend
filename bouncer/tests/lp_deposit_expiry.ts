#!/usr/bin/env pnpm tsx

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { exec } from 'child_process';
import { runWithTimeout, sleep } from '../shared/utils';

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
  const snowwhiteUri =
    process.env.SNOWWHITE_URI ??
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
  const snowwhite = keyring.createFromUri(snowwhiteUri);
  chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });

  console.log('=== Testing expiry of funded LP deposit address ===');
  console.log('Setting expiry time for LP addresses to 10 blocks');
  await chainflip.tx.governance
    .proposeGovernanceExtrinsic(chainflip.tx.liquidityProvider.setLpTtl(10))
    .signAndSend(snowwhite, { nonce: -1 });
  await observeEvent('liquidityProvider:LpTtlSet');
  console.log('Requesting new BTC LP deposit address');
  await chainflip.tx.liquidityProvider
    .requestLiquidityDepositAddress('Btc')
    .signAndSend(lp, { nonce: -1 });
  const ingressKey = (
    await observeEvent('liquidityProvider:LiquidityDepositAddressReady')
  )[1].toJSON().btc as string;
  let ingressAddress = '';
  for (let n = 2; n < ingressKey.length; n += 2) {
    ingressAddress += String.fromCharCode(parseInt(ingressKey.slice(n, 2), 16));
  }
  exec(
    './commands/fund_btc.sh ' + ingressAddress + ' 1',
    { timeout: 30000 },
    (err, stdout, stderr) => {
      if (stderr !== '') process.stdout.write(stderr);
      if (err !== null) {
        console.error(err);
        process.exit(1);
      }
      if (stdout !== '') process.stdout.write(stdout);
    },
  );
  await observeEvent('liquidityProvider:LiquidityDepositAddressExpired');
  console.log('Setting expiry time for LP addresses to 100 blocks');
  await chainflip.tx.governance
    .proposeGovernanceExtrinsic(chainflip.tx.liquidityProvider.setLpTtl(100))
    .signAndSend(snowwhite, { nonce: -1 });
  await observeEvent('liquidityProvider:LpTtlSet');
  console.log('=== Test complete ===');
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
