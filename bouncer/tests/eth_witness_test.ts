#!/usr/bin/env pnpm tsx
import { Keyring } from '@polkadot/keyring';
import { exec } from 'child_process';
import { runWithTimeout, observeEvent, getChainflipApi } from '../shared/utils';

async function main(): Promise<void> {
  const chainflip = await getChainflipApi();
  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI ?? '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  console.log('Requesting ETH deposit address');
  await chainflip.tx.liquidityProvider.requestLiquidityDepositAddress('Eth').signAndSend(lp);
  const ethIngressKey = (
    await observeEvent('liquidityProvider:LiquidityDepositAddressReady', chainflip)
  ).data.depositAddress.Eth as string;
  console.log(`Found ETH address: ${ethIngressKey}`);

  exec(
    `pnpm tsx ./commands/send_eth.ts ${ethIngressKey} 10`,
    { timeout: 20000 },
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
  console.log('Successfully witnessed transfer!');
  process.exit(0);
}

// Allow roughly 100 seconds (7 blocks safety margin, each block ~12s apart)
// for the witnesser to detect the deposit before considering it a failure
runWithTimeout(main(), 100000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
