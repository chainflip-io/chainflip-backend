import { InternalAsset as Asset } from '@chainflip/cli';
import { observeEvent, getChainflipApi, assetDecimals } from '../shared/utils';
import { submitGovernanceExtrinsic } from './cf_governance';

export async function createLpPool(ccy: Asset, initialPrice: number) {
  await using chainflip = await getChainflipApi();

  if (
    (
      await chainflip.query.liquidityPools.pools({
        assets: { quote: 'usdc', base: ccy.toLowerCase() },
      })
    ).toJSON()! === null
  ) {
    const price = BigInt(
      Math.round((initialPrice / 10 ** (assetDecimals(ccy) - assetDecimals('Usdc'))) * 2 ** 128),
    );
    console.log(
      'Setting up ' + ccy + ' pool with an initial price of ' + initialPrice + ' USDC per ' + ccy,
    );
    const poolCreatedEvent = observeEvent(
      'liquidityPools:NewPoolCreated',
      chainflip,
      (event) => event.data.baseAsset === ccy,
    );
    const extrinsic = chainflip.tx.liquidityPools.newPool(ccy, 'usdc', 20, price);
    await submitGovernanceExtrinsic(extrinsic);
    await poolCreatedEvent;
  }
}
