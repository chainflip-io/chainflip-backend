import { webSocket } from 'rxjs/webSocket';
import { Observable, from, merge } from 'rxjs';
import { filter, map, mergeMap } from 'rxjs/operators';
import WebSocket from 'ws';
import { lpApiRpc } from '../shared/json_rpc';
import { Asset, Chain, createStateChainKeypair, getContractAddress } from '../shared/utils';
import { globalLogger as logger } from '../shared/utils/logger';
import { sendErc20 } from '../shared/send_erc20';
import { Order, Side, OrderStatus, OrderType, Swap, TradeDecision } from './utils';
import { fromEvent } from 'rxjs';
import { EventEmitter } from 'events';

(global as any).WebSocket = WebSocket;

const customEmitter = new EventEmitter();

// Todo: Upgrade to a persistent state
declare global {
    var ORDER_BOOK: Map<number, Order>;
    var SWAPS: Map<number, Swap>;
    var LP_ACCOUNT: string;
    var CHAIN: string;
    var ASSET: string;
}

/**
 * Determines if a swap should be executed.
 * 
 * @param swap - The swap to evaluate.
 * @returns The trade decision.
 */
const tradingStrategy = (swap: Swap): TradeDecision => {
    return {
        shouldTrade: true,
        side: swap.side === Side.Sell ? Side.Buy : Side.Sell, // If someone sells we buy and vice versa
        asset: swap.baseAsset,
        amount: swap.amount,
        price: 0, // We always use the current pool price
    };
};

/**
 * Manages limit orders.
 * 
 * @param decision - The trade decision.
 * @returns The order ID.
 */
const manageLimitOrders = async (decision: TradeDecision) => {
    let orderId;

    const currentOpenOrderForAsset = Array.from(global.ORDER_BOOK.values()).find(order => order.asset === decision.asset && order.orderType === OrderType.Limit && order.amount >= decision.amount);

    if (currentOpenOrderForAsset) {
        logger.info(`Found existing order for asset: ${decision.asset}`);
        return currentOpenOrderForAsset.orderId;
    }

    orderId = Math.floor(Math.random() * 10000) + 1;

    try {
        await lpApiRpc(logger, 'lp_set_limit_order', [
            decision.asset,
            { chain: 'Ethereum', asset: 'USDC' },
            decision.side,
            orderId,
            0,
            decision.amount,
        ]);
        global.ORDER_BOOK.set(orderId, new Order(orderId, OrderStatus.Accepted, OrderType.Limit, decision.asset, decision.side, decision.amount, decision.price));
        return orderId;
    } catch (error) {
        logger.error(`Failed to execute order: ${error}`);
        customEmitter.emit('outOfLiquidity', { side: decision.side, asset: decision.asset, amount: decision.amount });
        return null;
    }
};

/**
 * Creates a swap stream.
 * 
 * @param wsConnection - The WebSocket connection.
 * @returns The swap stream.
 */
const createSwapStream = (wsConnection: any): Observable<Swap> => {
    return wsConnection.pipe(
        filter((msg: any) => msg.method === 'cf_subscribe_scheduled_swaps'),
        map((msg: any) => {
            return msg.params.result.swaps;
        }),
        mergeMap((swaps: any[]) => from(swaps)),
        map((swap: any): Swap => ({
            swapId: swap.swap_id,
            baseAsset: swap.base_asset,
            quoteAsset: swap.quote_asset,
            side: swap.side,
            amount: swap.amount
        })),
        filter((swap: Swap) => {
            if (global.SWAPS.has(swap.swapId)) {
                return false;
            }
            global.SWAPS.set(swap.swapId, swap);
            return true;
        })
    );
};

/**
 * Creates a trade decision stream.
 * 
 * @param swaps$ - The swap stream.
 * @returns The trade decision stream.
 */
const createTradeDecisionStream = (swaps$: Observable<Swap>): Observable<TradeDecision> => {
    return swaps$.pipe(
        map(tradingStrategy),
        filter(decision => decision.shouldTrade)
    );
};

/**
 * Creates an order stream.
 * 
 * @param decisions$ - The trade decision stream.
 * @returns The order stream.
 */
const createOrderStream = (decisions$: Observable<TradeDecision>): Observable<number | null> => {
    return decisions$.pipe(
        map(manageLimitOrders),
        mergeMap(orderId => from(orderId))
    );
};

/**
 * Creates an order fill stream.
 * 
 * @param wsConnection - The WebSocket connection.
 * @returns The order fill stream.
 */
const createOrderFillStream = (wsConnection: any): Observable<any> => {
    return wsConnection.pipe(
        filter((msg: any) => msg.method === 'lp_subscribe_order_fills'),
        map((msg: any) => {
            return msg.params.result.fills;
        }),
        mergeMap((fills: any[]) => from(fills))
    );
};

/** 
 * Deposits USDC liquidity.
 * 
 * @param amount - The amount of USDC to deposit.
 */
const depositLiquidity = async (chain: Chain, asset: Asset, amount: string) => {
    const contractAddress = getContractAddress(chain, asset);
    const liquidityDepositAddress = await lpApiRpc(logger, 'lp_liquidity_deposit', [{ chain, asset }, 'InBlock']);
    await sendErc20(logger, chain, liquidityDepositAddress.tx_details.response, contractAddress, amount);
    logger.info(`Liquidity deposited: ${amount} ${asset}!`);
}

/**
 * Creates a pool price stream.
 * 
 * @param wsConnection - The WebSocket connection.
 * @returns The pool price stream.
 */
const createPoolPriceStream = (wsConnection: any): Observable<any> => {
    return wsConnection.pipe(
        filter((msg: any) => msg.method === 'cf_subscribe_pool_price_v2'),
        map((msg: any) => msg.params.result)
    );
};

/**
 * Initializes the liquidity provider bot.
 * 
 * @returns The state chain and liquidity provider WebSocket connections.
 */
const initializeLiquidityProviderBot = (chain: string, asset: string) => {
    logger.info('Initializing liquidity provider bot 🤖.');

    global.ORDER_BOOK = new Map<number, Order>();
    global.SWAPS = new Map<number, Swap>();
    global.LP_ACCOUNT = createStateChainKeypair('//LP_API').address;
    global.CHAIN = chain;
    global.ASSET = asset;

    logger.info(`LP Account: ${global.LP_ACCOUNT}`);

    const stateChainWsConnection = webSocket('ws://127.0.0.1:9944');
    const lpWsConnection = webSocket('ws://127.0.0.1:10589');

    // Create our reactive pipeline
    const swaps$ = createSwapStream(stateChainWsConnection);
    const tradeDecisions$ = createTradeDecisionStream(swaps$);
    const orders$ = createOrderStream(tradeDecisions$);

    const orderFills$ = createOrderFillStream(lpWsConnection);
    const poolPrices$ = createPoolPriceStream(lpWsConnection);

    // Subscribe to the final stream to start the flow
    orders$.subscribe({
        next: (orderId) => {
            if (orderId) {
                logger.info(`Submitted order: ${orderId} successfully ✅.`);
            }
        },
        error: (err) => logger.error('Error in order stream:', err),
        complete: () => logger.info('Order stream completed')
    });

    swaps$.subscribe({
        next: (swap) => {
            logger.info(`Received new swap Id: ${swap.swapId}, amount: ${swap.amount}, side: ${swap.side}, baseAsset: ${swap.baseAsset}, quoteAsset: ${swap.quoteAsset} 🔄.`);
        },
        error: (err) => logger.error('Error in swap stream:', err),
        complete: () => logger.info('Swap stream completed')
    });

    stateChainWsConnection.next({
        id: 1,
        jsonrpc: "2.0",
        method: "cf_subscribe_scheduled_swaps",
        params: {
            base_asset: { chain: global.CHAIN, asset: global.ASSET },
            quote_asset: { chain: 'Ethereum', asset: 'USDC' }
        }
    });

    // Subscribe to order fills stream
    orderFills$.subscribe({
        next: (fill) => {
            if (fill.limit_order) {
                if (fill.limit_order.lp === global.LP_ACCOUNT) {
                    logger.info(`We won a swap 🕺 !`);
                    let idAsNumber = parseInt(fill.limit_order.id);
                    let order = global.ORDER_BOOK.get(idAsNumber)!;
                    order.status = OrderStatus.Filled;
                    global.ORDER_BOOK.set(idAsNumber, order);
                }
            }
        },
        error: (err) => logger.error('Error in order fills stream:', err),
        complete: () => logger.info('Order fills stream completed')
    });

    poolPrices$.subscribe({
        next: (poolPrice) => {
            logger.info(`Received pool price change: ${JSON.stringify(poolPrice, null, 2)} 📈.`);
        },
        error: (err) => logger.error('Error in pool price stream:', err),
        complete: () => logger.info('Pool price stream completed')
    });

    lpWsConnection.next({
        "id": 1,
        "jsonrpc": "2.0",
        "method": "lp_subscribe_order_fills",
        "params": []
    });

    lpWsConnection.next({
        "id": 1,
        "jsonrpc": "2.0",
        "method": "cf_subscribe_pool_price",
        "params": {
            "from_asset": {
                "chain": global.CHAIN,
                "asset": global.ASSET
            },
            "to_asset": {
                "chain": "Ethereum",
                "asset": "USDC"
            }
        }
    });

    const outOfLiquidity$ = fromEvent(customEmitter, 'outOfLiquidity');

    outOfLiquidity$.subscribe({
        next: (event) => {
            logger.info('Out of liquidity 💸!');
            logger.info(`Received event: ${JSON.stringify(event, null, 2)}`);
        }
    });

    return [stateChainWsConnection, lpWsConnection];
};

export { initializeLiquidityProviderBot };