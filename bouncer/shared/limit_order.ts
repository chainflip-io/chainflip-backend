import { InternalAsset as Asset } from '@chainflip/cli';
import {
  handleSubstrateError,
  amountToFineAmount,
  lpMutex,
  assetDecimals,
  createStateChainKeypair,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from './utils/substrate';

export async function limitOrder(
  ccy: Asset,
  amount: number,
  orderId: number,
  tick: number,
  lpKey?: string,
) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));
  await using chainflip = await getChainflipApi();

  const lp = createStateChainKeypair(lpKey ?? (process.env.LP_URI || '//LP_1'));

  console.log('Setting up ' + ccy + ' limit order');
  const orderCreatedEvent = observeEvent('liquidityPools:LimitOrderUpdated', {
    test: (event) =>
      event.data.lp === lp.address && event.data.baseAsset === ccy && event.data.id === String(0),
  }).event;
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .setLimitOrder(ccy.toLowerCase(), 'usdc', 'sell', orderId, tick, fineAmount)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await orderCreatedEvent;
}
