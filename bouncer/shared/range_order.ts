import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { assetDecimals, Asset } from '@chainflip-io/cli';
import {
  observeEvent,
  getChainflipApi,
  handleSubstrateError,
  amountToFineAmount,
  lpMutex,
} from '../shared/utils';

export async function rangeOrder(ccy: Asset, amount: number) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals[ccy]);
  const chainflip = await getChainflipApi();
  await cryptoWaitReady();

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
  const orderCreatedEvent = observeEvent(
    'liquidityPools:RangeOrderUpdated',
    chainflip,
    (event) =>
      event.data.lp === lp.address &&
      event.data.baseAsset.toUpperCase() === ccy &&
      event.data.id === String(0),
  );
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .setRangeOrder(ccy.toLowerCase(), 'usdc', 0, [-887272, 887272], {
        Liquidity: { Liquidity: liquidity },
      })
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await orderCreatedEvent;
}
