#!/usr/bin/env -S pnpm tsx
import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { exec } from 'child_process';
import {
  runWithTimeout,
  sleep,
  hexStringToBytesArray,
  newAddress,
  observeEvent,
} from '../shared/utils';

let chainflip: ApiPromise;

async function main(): Promise<void> {
  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI ?? '//LP_1';
  const lp = keyring.createFromUri(lpUri);
  chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
    types: {
      EncodedAddress: {
        _enum: {
          Eth: '[u8; 20]',
          Dot: '[u8; 32]',
          Btc: '[u8; 34]',
        },
      },
    },
  });

  // Register Emergency Withdrawal Address before requesting reposit address.
  const encodedEthAddr = chainflip.createType('EncodedAddress', {
    Eth: hexStringToBytesArray(await newAddress('ETH', 'LP_1')),
  });
  await chainflip.tx.liquidityProvider
    .registerEmergencyWithdrawalAddress(encodedEthAddr)
    .signAndSend(lp);

  await chainflip.tx.liquidityProvider.requestLiquidityDepositAddress('Eth').signAndSend(lp);
  const ethIngressKey = (
    await observeEvent(
      'liquidityProvider:LiquidityDepositAddressReady',
      chainflip,
      (event) => event.data.depositAddress.Eth,
    )
  ).data.depositAddress.Eth as string;
  console.log('ETH ingress address: ' + ethIngressKey);
  await sleep(8000); // sleep for 8 seconds to give the engine a chance to start witnessing
  exec(
    'pnpm tsx  ./commands/send_eth.ts ' + ethIngressKey + ' 10',
    { timeout: 10000 },
    (err, stdout, stderr) => {
      if (stderr !== '') process.stdout.write(stderr);
      if (err !== null) {
        console.error(err);
        process.exit(1);
      }
      if (stdout !== '') process.stdout.write(stdout);
    },
  );
  await observeEvent('liquidityProvider:AccountCredited', chainflip);
  exec(
    'pnpm tsx  ./commands/send_eth.ts ' + ethIngressKey + ' 10',
    { timeout: 10000 },
    (err, stdout, stderr) => {
      if (stderr !== '') process.stdout.write(stderr);
      if (err !== null) {
        console.error(err);
        process.exit(1);
      }
      if (stdout !== '') process.stdout.write(stdout);
    },
  );
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
