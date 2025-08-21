import { InternalAsset as Asset } from '@chainflip/cli';
import {
  waitForExt,
  amountToFineAmount,
  lpMutex,
  assetDecimals,
  createStateChainKeypair,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { Logger } from 'shared/utils/logger';

export async function limitOrder(
  logger: Logger,
  ccy: Asset,
  amount: number,
  orderId: number,
  tick: number,
  lpKey?: string,
) {
  const fineAmount = amountToFineAmount(String(amount), assetDecimals(ccy));
  await using chainflip = await getChainflipApi();

  const lpUri = lpKey ?? (process.env.LP_URI || '//LP_1');
  const lp = createStateChainKeypair(lpUri);

  logger.info('Setting up ' + ccy + ' limit order');
  const release = await lpMutex.acquire(lpUri);
  const { promise, waiter } = waitForExt(chainflip, logger, 'InBlock', release);
  const nonce = (await chainflip.rpc.system.accountNextIndex(lp.address)) as unknown as number;
  const unsub = await chainflip.tx.liquidityPools
    .setLimitOrder(ccy.toLowerCase(), 'usdc', 'sell', orderId, tick, fineAmount, null, null)
    .signAndSend(lp, { nonce }, waiter);
  await promise;
  unsub();
  logger.info(`Limit order for ${ccy} with ID ${orderId} set successfully`);
}
