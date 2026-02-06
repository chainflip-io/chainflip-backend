import { lpApiRpc } from 'shared/json_rpc';
import { Assets, createStateChainKeypair, stateChainAssetFromAsset } from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { getChainflipApi } from 'shared/utils/substrate';

export async function logLpPoolOrders(logger: Logger, lpAddress: string) {
  await using chainflip = await getChainflipApi();

  const allPoolAssets = Object.values(Assets).filter(
    (asset) => asset !== 'Usdc' && asset !== 'Dot',
  );

  for (const asset of allPoolAssets) {
    const assetAndChain = stateChainAssetFromAsset(asset);

    const orders = await chainflip.rpc('cf_pool_orders', assetAndChain, 'USDC', lpAddress);
    logger.info(`${asset} pool: ${JSON.stringify(orders)}`);
  }
}

export async function DeleteAllOrdersLpApi(logger: Logger) {
  const lp = createStateChainKeypair(process.env.LP_URI || '//LP_API');

  await logLpPoolOrders(logger, lp.address);

  logger.info(`Cancelling all lp orders for '//LP_API' ${lp.address} \n`);
  await lpApiRpc(logger, `lp_cancel_all_orders`, []);

  await logLpPoolOrders(logger, lp.address);
}
