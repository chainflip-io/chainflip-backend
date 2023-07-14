import { observeEvent, getChainflipApi, assetToDecimals } from '../shared/utils';
import { Asset } from '@chainflip-io/cli';
import { submitGovernanceExtrinsic } from './cf_governance';

export async function createLpPool(ccy: Asset, initial_price: number) {
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);

  const price = BigInt(
    Math.round(
      Math.sqrt(
        initial_price / Math.pow(10, assetToDecimals.get(ccy)! - assetToDecimals.get('USDC')!),
      ) * Math.pow(2, 96),
    ),
  );
  console.log(
    'Setting up ' + ccy + ' pool with an initial price of ' + initial_price + ' USDC per ' + ccy,
  );
  let event = observeEvent('liquidityPools:NewPoolCreated', chainflip, (data) => {
    return data[0].toUpperCase() == ccy;
  });
  const extrinsic = chainflip.tx.liquidityPools.newPool(ccy.toLowerCase(), 0, price);
  await submitGovernanceExtrinsic(extrinsic);
  await event;
}
