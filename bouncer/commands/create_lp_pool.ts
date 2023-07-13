// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a new liquidity pool for the given currency and
// initial price in USDC
// For example: pnpm tsx ./commands/create_pool.ts btc 10000

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset } from '@chainflip-io/cli';
import {
  observeEvent,
  getChainflipApi,
  handleSubstrateError,
  assetToDecimals,
  runWithTimeout,
} from '../shared/utils';

async function createLpPool() {
  const ccy = process.argv[2].toUpperCase() as Asset;
  const initialPrice = parseFloat(process.argv[3]);
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  const snowwhiteUri =
    process.env.SNOWWHITE_URI ||
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
  const snowwhite = keyring.createFromUri(snowwhiteUri);

  const price = BigInt(
    Math.round(
      Math.sqrt(initialPrice / 10 ** (assetToDecimals.get(ccy)! - assetToDecimals.get('USDC')!)) *
        2 ** 96,
    ),
  );
  console.log(
    'Setting up ' + ccy + ' pool with an initial price of ' + initialPrice + ' USDC/' + ccy,
  );
  const event = observeEvent(
    'liquidityPools:NewPoolCreated',
    chainflip,
    (data) => data[0].toUpperCase() === ccy,
  );
  await chainflip.tx.governance
    .proposeGovernanceExtrinsic(chainflip.tx.liquidityPools.newPool(ccy.toLowerCase(), 0, price))
    .signAndSend(snowwhite, { nonce: -1 }, handleSubstrateError(chainflip));
  await event;
  process.exit(0);
}

runWithTimeout(createLpPool(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
