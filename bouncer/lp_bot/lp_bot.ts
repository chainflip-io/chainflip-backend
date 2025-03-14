import { webSocket } from 'rxjs/webSocket';
import { Observable, from, merge } from 'rxjs';
import { filter, map, mergeMap } from 'rxjs/operators';
import WebSocket from 'ws';
import { lpApiRpc } from '../shared/json_rpc';
import { Asset, getContractAddress } from '../shared/utils';
import { globalLogger as logger } from '../shared/utils/logger';
import { sendErc20 } from '../shared/send_erc20';

(global as any).WebSocket = WebSocket;

/**
 * The status of an order.
 */
enum OrderStatus {
    Submitted = 'submitted',
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

// Todo: Upgrade to a persistent state
declare global {
    var ORDER_BOOK: Map<number, Order>;
    var SWAPS: Map<number, Swap>;
}

/**
 * Determines if a swap should be executed.
 * 
 * @param swap - The swap to evaluate.
 * @returns The trade decision.
 */
const tradingStrategy = (swap: Swap): TradeDecision => {

    // Only handle USDT pairs
    if (swap.baseAsset.asset !== 'USDT') {
        return { shouldTrade: false, side: swap.side, asset: swap.baseAsset, amount: 0, price: 0 };
    }

    return {
        shouldTrade: true,
        side: swap.side === Side.Sell ? Side.Buy : Side.Sell, // If someone sells we buy and vice versa
        asset: swap.baseAsset,
        amount: swap.amount,
        price: 0,
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
    let orderPayload;
    let setOrUpdate = 'SET';

    const currentOpenOrderForAsset = Array.from(global.ORDER_BOOK.values()).find(order => order.asset === decision.asset && order.orderType === OrderType.Limit);

    if (currentOpenOrderForAsset) {
        let order = global.ORDER_BOOK.get(currentOpenOrderForAsset.orderId)!;
        let lastPrice = order.price;
        orderId = order.orderId;
        order.amount = decision.amount;
        order.price = decision.price;
        setOrUpdate = 'UPDATE';
    } else {
        orderId = Math.floor(Math.random() * 10000) + 1;
        global.ORDER_BOOK.set(orderId, new Order(orderId, OrderStatus.Submitted, OrderType.Limit, decision.asset, decision.side, decision.amount, decision.price));
    }

    try {
        logger.info(`Executing ${setOrUpdate} order: ${JSON.stringify(orderPayload, null, 2)}`);
        await lpApiRpc(logger, setOrUpdate === 'SET' ? 'lp_set_limit_order' : 'lp_update_limit_order', [
            decision.asset,
            { chain: 'Ethereum', asset: 'USDC' },
            decision.side,
            orderId,
            0,
            decision.amount,
        ]);
        return orderId;
    } catch (error) {
        logger.error(`Failed to execute order: ${error}`);
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
            logger.info(`Swap received: ${JSON.stringify(msg.params.result.swaps, null, 2)}`);
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
            if (msg.params.result.fills.length > 0) {
                logger.info(`Order fills received: ${JSON.stringify(msg.params.result.fills, null, 2)}`);
            }
            return msg.params.result.fills;
        }),
        mergeMap((fills: any[]) => from(fills))
    );
};

const manageRangeOrder = async (baseAsset: Asset, tick1: number, tick2: number, size: number) => {
    logger.info(`Managing range order for ${baseAsset} with tick1: ${tick1}, tick2: ${tick2}, size: ${size}`);
    let orderId = Math.floor(Math.random() * 10000) + 1;
    global.ORDER_BOOK.set(orderId, new Order(orderId, OrderStatus.Submitted, OrderType.Range, baseAsset, Side.Sell, size, tick1));
    try {
        await lpApiRpc(logger, 'lp_set_range_order', [
            baseAsset,
            { chain: 'Ethereum', asset: 'USDC' },
            orderId,
            [tick1, tick2],
            size,
        ]);
        logger.info(`Range order set: ${orderId}`);
    } catch (error) {
        logger.error(`Failed to execute order: ${error}`);
    }
}

/** 
 * Deposits USDC liquidity.
 * 
 * @param amount - The amount of USDC to deposit.
 */
const depositUsdcLiquidity = async (amount: string) => {
    const contractAddress = getContractAddress('Ethereum', 'Usdc');
    const liquidityDepositAddress = await lpApiRpc(logger, 'lp_liquidity_deposit', [{ chain: 'Ethereum', asset: 'USDC' }, 'InBlock']);
    await sendErc20(logger, 'Ethereum', liquidityDepositAddress.tx_details.response, contractAddress, amount);
    logger.info(`Liquidity deposited: ${amount} USDC!`);
}

/**
 * Cancels all orders.
 */
const cancelAllOrders = async () => {
    logger.info("Cancelling all orders");
    await lpApiRpc(logger, 'lp_cancel_all_orders', []);
}

/**
 * Initializes the liquidity provider bot.
 * 
 * @returns The state chain and liquidity provider WebSocket connections.
 */
const initializeLiquidityProviderBot = () => {
    logger.info('Initializing liquidity provider bot');

    global.ORDER_BOOK = new Map<number, Order>();
    global.SWAPS = new Map<number, Swap>();

    const stateChainWsConnection = webSocket('ws://127.0.0.1:9944');
    const lpWsConnection = webSocket('ws://127.0.0.1:10589');

    logger.info('Setting up reactive pipeline');

    // Create our reactive pipeline
    const swaps$ = createSwapStream(stateChainWsConnection);
    const tradeDecisions$ = createTradeDecisionStream(swaps$);
    const orders$ = createOrderStream(tradeDecisions$);
    const orderFills$ = createOrderFillStream(lpWsConnection);

    // Subscribe to the final stream to start the flow
    orders$.subscribe({
        next: (orderId) => {
            if (orderId) {
                logger.info(`Submitted order: ${orderId} successfully!`);
                const order = global.ORDER_BOOK.get(orderId)!;
                order.status = OrderStatus.Submitted;
                logger.info(`OrderBook State: `);
                console.log(global.ORDER_BOOK);
            }
        },
        error: (err) => logger.error('Error in order stream:', err),
        complete: () => logger.info('Order stream completed')
    });

    stateChainWsConnection.next({
        id: 1,
        jsonrpc: "2.0",
        method: "cf_subscribe_scheduled_swaps",
        params: {
            base_asset: { chain: "Ethereum", asset: "USDT" },
            quote_asset: { chain: "Ethereum", asset: "USDC" }
        }
    });

    // Subscribe to order fills stream
    orderFills$.subscribe({
        next: (fill) => {
            logger.info(`Received order fill: ${JSON.stringify(fill, null, 2)}`);
        },
        error: (err) => logger.error('Error in order fills stream:', err),
        complete: () => logger.info('Order fills stream completed')
    });

    lpWsConnection.next({
        "id": 1,
        "jsonrpc": "2.0",
        "method": "lp_subscribe_order_fills",
        "params": []
    });

    return [stateChainWsConnection, lpWsConnection];
};

export { initializeLiquidityProviderBot, depositUsdcLiquidity, cancelAllOrders, manageRangeOrder };