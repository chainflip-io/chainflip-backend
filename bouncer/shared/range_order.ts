import { InternalAsset as Asset } from '@chainflip/cli';
import {
  handleSubstrateError,
  amountToFineAmount,
  lpMutex,
  assetDecimals,
  createStateChainKeypair,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from './utils/substrate';

export async function rangeOrder(ccy: Asset | 'HubDot', amount: number, lpKey?: string, orderId?: number) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));
  await using chainflip = await getChainflipApi();

  const lp = createStateChainKeypair(lpKey ?? (process.env.LP_URI || '//LP_1'));

  /* eslint-disable @typescript-eslint/no-explicit-any */
  const currentPools: any = (
    await chainflip.query.liquidityPools.pools({
      assets: { quote: 'usdc', base: ccy.toLowerCase() },
    })
  ).toJSON();
  const currentSqrtPrice = currentPools!.poolState.rangeOrders.currentSqrtPrice;
  const liquidity = BigInt(Math.round((currentSqrtPrice / 2 ** 96) * Number(fineAmount)));
  console.log('Setting up ' + ccy + ' range order');
  const orderCreatedEvent = observeEvent('liquidityPools:RangeOrderUpdated', {
    test: (event) =>
      event.data.lp === lp.address &&
      event.data.baseAsset === ccy &&
      event.data.id === String(orderId || 0),
  }).event;
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .setRangeOrder(ccy.toLowerCase(), 'usdc', orderId || 0, [-887272, 887272], {
        Liquidity: { Liquidity: liquidity },
      })
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await orderCreatedEvent;
}
