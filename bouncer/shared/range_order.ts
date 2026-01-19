import { InternalAsset as Asset } from '@chainflip/cli';
import {
  amountToFineAmount,
  cfMutex,
  assetDecimals,
  createStateChainKeypair,
  waitForExt,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { Logger } from 'shared/utils/logger';

export async function rangeOrder(
  logger: Logger,
  ccy: Asset,
  amount: number,
  lpUri = process.env.LP_URI || '//LP_1',
  orderId = 0,
) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));
  await using chainflip = await getChainflipApi();

  const lp = createStateChainKeypair(lpUri);

  /* eslint-disable @typescript-eslint/no-explicit-any */
  const currentPools = (
    (await chainflip.query.liquidityPools.pools({
      assets: { quote: 'usdc', base: ccy.toLowerCase() },
    })) as unknown as any
  ).toJSON();
  const currentSqrtPrice = currentPools!.poolState.rangeOrders.currentSqrtPrice;
  const liquidity = BigInt(Math.round((currentSqrtPrice / 2 ** 96) * Number(fineAmount)));
  logger.info('Setting up ' + ccy + ' range order');
  const release = await cfMutex.acquire(lpUri);
  const { promise, waiter } = waitForExt(chainflip, logger, 'InBlock', release);
  const nonce = (await chainflip.rpc.system.accountNextIndex(lp.address)) as unknown as number;
  const unsub = await chainflip.tx.liquidityPools
    .setRangeOrder(ccy.toLowerCase(), 'usdc', orderId, [-887272, 887272], {
      Liquidity: { Liquidity: liquidity },
    })
    .signAndSend(lp, { nonce }, waiter);
  await promise;
  unsub();
  logger.info(`Range order for ${ccy} with amount ${amount} successfully set up`);
}
