import { amountToFineAmount, assetDecimals, Asset } from 'shared/utils';
import { getChainflipPolkadotApi } from 'shared/utils/substrate';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';

export async function rangeOrder<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  ccy: Asset,
  amount: number,
  orderId = 0,
) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));
  await using chainflip = await getChainflipPolkadotApi();

  /* eslint-disable @typescript-eslint/no-explicit-any */
  const currentPools = (
    (await chainflip.query.liquidityPools.pools({
      assets: { quote: 'usdc', base: ccy.toLowerCase() },
    })) as unknown as any
  ).toJSON();
  const currentSqrtPrice = currentPools!.poolState.rangeOrders.currentSqrtPrice;
  const liquidity = BigInt(Math.round((currentSqrtPrice / 2 ** 96) * Number(fineAmount)));

  cf.debug('Setting up ' + ccy + ' range order');

  await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityPools.setRangeOrder(
        ccy,
        'Usdc',
        BigInt(orderId),
        { start: -887272, end: 887272 },
        { type: 'Liquidity', value: { liquidity } },
      ),
  });

  cf.debug(`Range order for ${ccy} with amount ${amount} successfully set up`);
}
