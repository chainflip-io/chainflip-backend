// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a zero to infinity range order for the currency and amount given
// For example: pnpm tsx ./commands/range_order.ts btc 10

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import {
  observeEvent,
  getChainflipApi,
  runWithTimeout,
  handleSubstrateError,
  assetToDecimals,
  amountToFineAmount,
} from '../shared/utils';
import { Asset } from '@chainflip-io/cli';

const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';

async function range_order() {
  const ccy = process.argv[2].toUpperCase() as Asset;
  const amount = process.argv[3].trim();
  const fine_amount = amountToFineAmount(amount, assetToDecimals.get(ccy)!);
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const lp_uri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lp_uri);

  const current_sqrt_price = (
    await chainflip.query.liquidityPools.pools(ccy.toLowerCase())
  ).toJSON()!.poolState.rangeOrders.currentSqrtPrice;
  const price = Math.round((current_sqrt_price / Math.pow(2, 96)) * Number(fine_amount));
  console.log('Setting up ' + ccy + ' range order');
  const event = observeEvent('liquidityPools:RangeOrderMinted', chainflip, (data) => {
    return data[0] == lp.address && data[1].toUpperCase() == ccy;
  });
  await chainflip.tx.liquidityPools
    .collectAndMintRangeOrder(ccy.toLowerCase(), [-887272, 887272], price)
    .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  await event;
  process.exit(0);
}

runWithTimeout(range_order(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
