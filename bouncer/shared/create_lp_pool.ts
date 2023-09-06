import { assetDecimals, Asset } from '@chainflip-io/cli';
import { observeEvent, getChainflipApi } from '../shared/utils';
import { submitGovernanceExtrinsic } from './cf_governance';

export async function createLpPool(ccy: Asset, initialPrice: number) {
  const chainflip = await getChainflipApi();

  if (
    (
      await chainflip.query.liquidityPools.pools({
        assets: { one: 'usdc', zero: ccy.toLowerCase() },
      })
    ).toJSON()! === null
  ) {
    const price = BigInt(
      Math.round((initialPrice / 10 ** (assetDecimals[ccy] - assetDecimals.USDC)) * 2 ** 128),
    );
    console.log(
      'Setting up ' + ccy + ' pool with an initial price of ' + initialPrice + ' USDC per ' + ccy,
    );
    const poolCreatedEvent = observeEvent(
      'liquidityPools:NewPoolCreated',
      chainflip,
      (event) => event.data.pairAsset.toUpperCase() === ccy,
    );
    const extrinsic = chainflip.tx.liquidityPools.newPool('usdc', ccy.toLowerCase(), 0, price);
    await submitGovernanceExtrinsic(extrinsic);
    await poolCreatedEvent;
  }
}
