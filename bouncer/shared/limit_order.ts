import { InternalAsset as Asset } from '@chainflip/cli';
import { amountToFineAmount, assetDecimals } from 'shared/utils';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';

export async function limitOrder<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  ccy: Asset,
  amount: number,
  orderId: number,
  tick: number,
) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));

  cf.info('Setting up ' + ccy + ' limit order');

  await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityPools.setLimitOrder(
        ccy.toLowerCase(),
        'usdc',
        'sell',
        orderId,
        tick,
        fineAmount,
        null,
        null,
      ),
  });

  cf.info(`Limit order for ${ccy} with ID ${orderId} set successfully`);
}
