import { getChainflipApi } from '../shared/utils/substrate';
import { createStateChainKeypair } from "../shared/utils";
import { handleSubstrateError } from "../shared/utils";
import { globalLogger as logger } from '../shared/utils/logger';

/**
 * The status of an order.
 */
enum OrderStatus {
    Accepted = 'accepted',
    Filled = 'filled',
    Cancelled = 'cancelled'
}

/**
 * The type of an order.
 */
enum OrderType {
    Limit = 'limit',
    Range = 'range'
}

/**
 * An order.
 */
class Order {
    constructor(
        public orderId: number,
        public status: OrderStatus,
        public orderType: OrderType,
        public asset: Asset,
        public side: Side,
        public amount: number,
        public price: number,
    ) { }
}

/**
 * The side of an order.
 */
enum Side {
    Buy = 'buy',
    Sell = 'sell'
}

/**
 * A swap.
 */
type Swap = {
    swapId: number;
    baseAsset: { chain: string; asset: string };
    quoteAsset: { chain: string; asset: string };
    side: Side;
    amount: number;
}

/**
 * A trade decision.
 */
type TradeDecision = {
    shouldTrade: boolean;
    side: Side;
    asset: Asset;
    amount: number;
    price: number;
}

/**
 * Cancels all orders for a given liquidity provider.
 * 
 * @param lpAccount - The account of the liquidity provider.
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

export { cancelAllOrdersForLp, Order, Side, OrderStatus, OrderType, Swap, TradeDecision };