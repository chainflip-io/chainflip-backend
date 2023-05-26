#!/usr/bin/env pnpm tsx

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { exec } from 'child_process';
import { runWithTimeout, sleep, observeEvent } from '../shared/utils';

let chainflip: ApiPromise;

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
  await observeEvent('liquidityProvider:LpTtlSet', chainflip);
  console.log('Requesting new BTC LP deposit address');
  await chainflip.tx.liquidityProvider
    .requestLiquidityDepositAddress('Btc')
    .signAndSend(lp, { nonce: -1 });

  const depositEventResult = await observeEvent('liquidityProvider:LiquidityDepositAddressReady', chainflip);
  console.log('Received BTC LP deposit address: ' + depositEventResult);
  const ingressKey = depositEventResult[1].toJSON().btc;

  let ingressAddress = '';
  for (let n = 2; n < ingressKey.length; n += 2) {
    ingressAddress += String.fromCharCode(parseInt(ingressKey.slice(n, n + 2), 16));
  }

  console.log('Funding BTC LP deposit address of ' + ingressAddress + ' with 1 BTC');
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
  await observeEvent('liquidityProvider:LiquidityDepositAddressExpired', chainflip);
  console.log('Setting expiry time for LP addresses to 100 blocks');
  await chainflip.tx.governance
    .proposeGovernanceExtrinsic(chainflip.tx.liquidityProvider.setLpTtl(100))
    .signAndSend(snowwhite, { nonce: -1 });
  await observeEvent('liquidityProvider:LpTtlSet', chainflip);
  console.log('=== Test complete ===');
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
