import { InternalAsset as Asset } from '@chainflip/cli';
import { Keyring } from '../polkadot/keyring';
import { handleSubstrateError, amountToFineAmount, lpMutex, assetDecimals } from '../shared/utils';
import { getChainflipApi, observeEvent } from './utils/substrate';

export async function rangeOrder(ccy: Asset, amount: number) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));
  await using chainflip = await getChainflipApi();

  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const lpUri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  const currentSqrtPrice = (
    await chainflip.query.liquidityPools.pools({
      assets: { quote: 'usdc', base: ccy.toLowerCase() },
    })
  ).toJSON()!.poolState.rangeOrders.currentSqrtPrice;
  const liquidity = BigInt(Math.round((currentSqrtPrice / 2 ** 96) * Number(fineAmount)));
  console.log('Setting up ' + ccy + ' range order');
  const orderCreatedEvent = observeEvent('liquidityPools:RangeOrderUpdated', {
    test: (event) =>
      event.data.lp === lp.address && event.data.baseAsset === ccy && event.data.id === String(0),
  }).event;
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .setRangeOrder(ccy.toLowerCase(), 'usdc', 0, [-887272, 887272], {
        Liquidity: { Liquidity: liquidity },
      })
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await orderCreatedEvent;
}
