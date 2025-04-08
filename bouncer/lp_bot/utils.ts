import { getChainflipApi } from '../shared/utils/substrate';
import { createStateChainKeypair } from "../shared/utils";
import { handleSubstrateError } from "../shared/utils";
import { globalLogger as logger } from '../shared/utils/logger';

/**
 * Cancels all orders.
 */
const cancelAllOrdersForLp = async (lpAccount: string) => {
    await using chainflip = await getChainflipApi();
    const lp = createStateChainKeypair(lpAccount);

    logger.info(`Try to close all orders for: ${lp.address}...`);
    try {
        const orders = await chainflip.rpc('cf_pool_orders', { chain: "Ethereum", asset: "USDT" }, "USDC", lp.address);

        if (orders?.range_orders.length === 0 && orders?.limit_orders.asks.length === 0 && orders?.limit_orders.bids.length === 0) {
            logger.info(`No open orders found for: ${lp.address}`);
            return;
        }

        logger.info(`Open orders: ${JSON.stringify(orders, null, 2)}`);

        const orderToDelete: {
            Limit?: { base_asset: string; quote_asset: string; side: string; id: number };
            Range?: { base_asset: string; quote_asset: string; id: number };
        }[] = [];

        for (const order of orders?.range_orders) {
            console.log(order);
            orderToDelete.push({
                Range: {
                    base_asset: 'USDT',
                    quote_asset: 'USDC',
                    id: order.id
                }
            });
        };

        for (const order of orders?.limit_orders.asks) {
            console.log(order);
            orderToDelete.push({
                Limit: { base_asset: 'USDT', quote_asset: 'USDC', side: 'sell', id: order.id }
            });
        };

        for (const order of orders?.limit_orders.bids) {
            console.log(order);
            orderToDelete.push({
                Limit: { base_asset: 'USDT', quote_asset: 'USDC', side: 'buy', id: order.id }
            });
        };

        try {
            await chainflip.tx.liquidityPools
                .cancelOrdersBatch(orderToDelete)
                .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
            logger.info(`Orders cancelled: ${JSON.stringify(orderToDelete, null, 2)}`);
        } catch (error) {
            logger.error(`Error: ${error}`);
        }
    } catch (error) {
        logger.error(`Error: ${error}`);
    }
}

export { cancelAllOrdersForLp };