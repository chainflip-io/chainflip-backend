import { Asset } from '@chainflip-io/cli';
import { observeEvent, getChainflipApi, assetToDecimals } from '../shared/utils';
import { submitGovernanceExtrinsic } from './cf_governance';

export async function createLpPool(ccy: Asset, initialPrice: number) {
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);

  const price = BigInt(
    Math.round(
      Math.sqrt(initialPrice / 10 ** (assetToDecimals.get(ccy)! - assetToDecimals.get('USDC')!)) *
        2 ** 96,
    ),
  );
  console.log(
    'Setting up ' + ccy + ' pool with an initial price of ' + initialPrice + ' USDC per ' + ccy,
  );
  const event = observeEvent(
    'liquidityPools:NewPoolCreated',
    chainflip,
    (data) => data[0].toUpperCase() === ccy,
  );
  const extrinsic = chainflip.tx.liquidityPools.newPool(ccy.toLowerCase(), 0, price);
  await submitGovernanceExtrinsic(extrinsic);
  await event;
}
