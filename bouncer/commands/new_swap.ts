#!/usr/bin/env pnpm tsx

// INSTRUCTIONS
//
// This command takes four arguments.
// It will request a new swap with the provided parameters
// Argument 1 is the source currency ("btc", "eth", "dot" or "usdc")
// Argument 2 is the destination currency ("btc", "eth", "dot" or "usdc")
// Argument 3 is the destination address
// Argument 4 is the broker fee in basis points
// For example: ./commands/new_swap.ts dot btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX 100

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { u8aToHex } from '@polkadot/util';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const brokerUri = process.env.BROKER_URI ?? '//BROKER_1';
  const broker = keyring.createFromUri(brokerUri);
  const chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });
  const sourceCcy = process.argv[2];
  const destinationCcy = process.argv[3];
  const destinationAddress =
    destinationCcy === 'dot' ? u8aToHex(keyring.decodeAddress(process.argv[4])) : process.argv[4];
  const fee = process.argv[5];

  console.log('Requesting Swap ' + sourceCcy + ' -> ' + destinationCcy);
  await chainflip.tx.swapping
    .requestSwapDepositAddress(
      sourceCcy,
      destinationCcy,
      { [destinationCcy === 'usdc' ? 'eth' : destinationCcy]: destinationAddress },
      fee,
      null,
    )
    .signAndSend(broker);
  process.exit(0);
}

runWithTimeout(main(), 60000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
