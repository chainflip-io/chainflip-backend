#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund liquidity of the given currency and amount
// For example: pnpm ./commands/provide_liquidity.ts btc 1.5

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset } from '@chainflip-io/cli/.';
import {
  observeEvent,
  getChainflipApi,
  runWithTimeout,
  handleSubstrateError,
  encodeBtcAddressForContract,
} from '../shared/utils';
import { send } from '../shared/send';

const chain = new Map<Asset, string>([
  ['DOT', 'dot'],
  ['ETH', 'eth'],
  ['BTC', 'btc'],
  ['USDC', 'eth'],
  ['FLIP', 'eth'],
]);

async function main() {
  const ccy = process.argv[2].toUpperCase() as Asset;
  const amount = process.argv[3];
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.SNOWWHITE_URI || '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  console.log('Requesting ' + ccy + ' deposit address');
  let event = observeEvent(
    'liquidityProvider:LiquidityDepositAddressReady',
    chainflip,
    (data) => data[1][chain.get(ccy)!] !== undefined,
  );
  await chainflip.tx.liquidityProvider
    .requestLiquidityDepositAddress(ccy.toLowerCase())
    .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  let ingressAddress = (await event).depositAddress.toJSON()[chain.get(ccy)!];
  if (ccy === 'BTC') {
    ingressAddress = encodeBtcAddressForContract(ingressAddress);
  }
  console.log('Received ' + ccy + ' address: ' + ingressAddress);
  console.log('Sending ' + amount + ' ' + ccy + ' to ' + ingressAddress);
  event = observeEvent(
    'liquidityProvider:AccountCredited',
    chainflip,
    (data) => data[1].toUpperCase() === ccy,
  );
  send(ccy, ingressAddress, amount);
  await event;
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
