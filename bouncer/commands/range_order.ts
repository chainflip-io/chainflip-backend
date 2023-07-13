// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a zero to infinity range order for the currency and amount given
// For example: pnpm tsx ./commands/range_order.ts btc 10

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset } from '@chainflip-io/cli';
import {
  observeEvent,
  getChainflipApi,
  runWithTimeout,
  handleSubstrateError,
  assetToDecimals,
  amountToFineAmount,
} from '../shared/utils';

async function rangeOrder() {
  const ccy = process.argv[2].toUpperCase() as Asset;
  const amount = process.argv[3].trim();
  const fineAmount = amountToFineAmount(amount, assetToDecimals.get(ccy)!);
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const lpUri = process.env.lpUri || '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  const currentSqrtPrice = (await chainflip.query.liquidityPools.pools(ccy.toLowerCase())).toJSON()!
    .poolState.rangeOrders.currentSqrtPrice;
  const price = Math.round((currentSqrtPrice / 2 ** 96) * Number(fineAmount));
  console.log('Setting up ' + ccy + ' range order');
  const event = observeEvent(
    'liquidityPools:RangeOrderMinted',
    chainflip,
    (data) => data[0] === lp.address && data[1].toUpperCase() === ccy,
  );
  await chainflip.tx.liquidityPools
    .collectAndMintRangeOrder(ccy.toLowerCase(), [-887272, 887272], price)
    .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  await event;
  process.exit(0);
}

runWithTimeout(rangeOrder(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
