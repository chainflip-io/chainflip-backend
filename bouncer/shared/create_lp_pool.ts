import { assetDecimals, Asset } from '../shared/utils';
import { submitGovernanceExtrinsic } from './cf_governance';
import { getChainflipApi, observeEvent } from './utils/substrate';

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
    const poolCreatedEvent = observeEvent('liquidityPools:NewPoolCreated', {
      test: (event) => event.data.baseAsset === ccy,
    }).event;
    await submitGovernanceExtrinsic((api) => api.tx.liquidityPools.newPool(ccy, 'usdc', 20, price));
    await poolCreatedEvent;
  }
}
