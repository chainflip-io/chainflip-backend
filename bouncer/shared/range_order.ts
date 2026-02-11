import { InternalAsset as Asset } from '@chainflip/cli';
import { amountToFineAmount, assetDecimals } from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';

export async function rangeOrder<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  ccy: Asset,
  amount: number,
  orderId = 0,
) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));
  await using chainflip = await getChainflipApi();

  /* eslint-disable @typescript-eslint/no-explicit-any */
  const currentPools = (
    (await chainflip.query.liquidityPools.pools({
      assets: { quote: 'usdc', base: ccy.toLowerCase() },
    })) as unknown as any
  ).toJSON();
  const currentSqrtPrice = currentPools!.poolState.rangeOrders.currentSqrtPrice;
  const liquidity = BigInt(Math.round((currentSqrtPrice / 2 ** 96) * Number(fineAmount)));

  cf.info('Setting up ' + ccy + ' range order');

  await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityPools.setRangeOrder(ccy.toLowerCase(), 'usdc', orderId, [-887272, 887272], {
        Liquidity: { Liquidity: liquidity },
      }),
  });

  cf.info(`Range order for ${ccy} with amount ${amount} successfully set up`);
}
