#!/usr/bin/env -S pnpm tsx
import { webSocket, WebSocketSubject } from 'rxjs/webSocket';
import { Subject, Observable, from, merge } from 'rxjs';
import { filter, map, mergeMap } from 'rxjs/operators';
import WebSocket from 'ws';
import { lpApiRpc } from '../shared/json_rpc';
import { amountToFineAmount, Asset, assetDecimals, chainFromAsset, getContractAddress } from '../shared/utils';
import { stateChainAssetFromAsset } from '../shared/utils';
import { globalLogger as logger } from '../shared/utils/logger';
import { sendErc20 } from '../shared/send_erc20';

(global as any).WebSocket = WebSocket;

enum OrderStatus {
    Submitted = 'submitted',
    Accepted = 'accepted',
    Filled = 'filled',
    Cancelled = 'cancelled'
}

class Order {
    constructor(
        public orderId: number,
        public status: OrderStatus,
        public asset: Asset,
        public side: Side,
        public amount: number,
        public price: number,
    ) { }
}

enum Side {
    Buy = 'buy',
    Sell = 'sell'
}

type Swap = {
    swapId: number;
    baseAsset: { chain: string; asset: string };
    quoteAsset: { chain: string; asset: string };
    side: Side;
    amount: number;
}

type TradeDecision = {
    shouldTrade: boolean;
    side: Side;
    asset: Asset;
    amount: number;
    price: number;
}

const ORDER_BOOK = new Map<number, Order>();
const SWAPS = new Map<number, Swap>();

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

// Pure function for order creation
const createOrderPayload = (decision: TradeDecision) => {
    const orderId = Math.floor(Math.random() * 10000) + 1;
    return {
        orderId,
        params: [
            decision.asset,
            { chain: 'Ethereum', asset: 'USDC' },
            decision.side,
            orderId,
            0,
            decision.amount,
        ]
    };
};

const transmitOrder = async (orderPayload: ReturnType<typeof createOrderPayload>) => {
    try {
        await lpApiRpc(logger, 'lp_set_limit_order', orderPayload.params);
        return orderPayload.orderId;
    } catch (error) {
        logger.error(`Failed to execute order: ${error}`);
        return null;
    }
};

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
            if (SWAPS.has(swap.swapId)) {
                return false;
            }
            SWAPS.set(swap.swapId, swap);
            return true;
        })
    );
};

const manageOrders = (decision: TradeDecision) => {
    // Check if there is an order already in the order book and if yes get the amount and price
    const ordersForAsset = Array.from(ORDER_BOOK.values()).filter(
        order => order.asset === decision.asset &&
            order.status === OrderStatus.Submitted
    );

    // If there is an order in the order book, we need to update the amount and price
    if (ordersForAsset.length > 0) {
        const order = ordersForAsset[0];
        order.amount = decision.amount;
        order.price = decision.price;
    }

}

const createTradeDecisionStream = (swaps$: Observable<Swap>): Observable<TradeDecision> => {
    return swaps$.pipe(
        map(tradingStrategy),
        filter(decision => decision.shouldTrade)
    );
};

const createOrderStream = (decisions$: Observable<TradeDecision>): Observable<number | null> => {
    return decisions$.pipe(
        map(createOrderPayload),
        mergeMap(orderPayload => from(transmitOrder(orderPayload)))
    );
};

const depositUsdcLiquidity = async (amount: string) => {
    const contractAddress = getContractAddress('Ethereum', 'Usdc');
    const liquidityDepositAddress = await lpApiRpc(logger, 'lp_liquidity_deposit', [{ chain: 'Ethereum', asset: 'USDC' }, 'InBlock']);
    await sendErc20(logger, 'Ethereum', liquidityDepositAddress.tx_details.response, contractAddress, amount);
    logger.info(`Liquidity deposited: ${amount} USDC!`);
}

const cancelAllOrders = async () => {
    logger.info("Cancelling all orders");
    await lpApiRpc(logger, 'lp_cancel_all_orders', []);
}

// WebSocket setup and subscription handling
const initializeLiquidityProviderBot = () => {
    logger.info('Initializing liquidity provider bot');
    const stateChainWsConnection = webSocket('ws://127.0.0.1:9944');
    const lpWsConnection = webSocket('ws://127.0.0.1:10589');

    logger.info('Setting up reactive pipeline');
    // Create our reactive pipeline
    const swaps$ = createSwapStream(stateChainWsConnection);
    const tradeDecisions$ = createTradeDecisionStream(swaps$);
    const orders$ = createOrderStream(tradeDecisions$);

    // Subscribe to the final stream to start the flow
    orders$.subscribe({
        next: (orderId) => {
            if (orderId) {
                logger.info(`Submitted order: ${orderId} successfully!`);
                ORDER_BOOK.set(orderId, OrderStatus.Submitted);
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

    lpWsConnection.next(
        {
            "id": 1,
            "jsonrpc": "2.0",
            "method": "lp_subscribe_order_fills",
            "params": []
        }
    );

    lpWsConnection.subscribe({
        next: (msg: any) => {
            if (msg.method === 'lp_subscribe_order_fills') {
                if (msg.params.result.fills.length > 0) {
                    logger.info(`Order fills received: ${JSON.stringify(msg.params.result.fills, null, 2)}`);
                }
            }
        }
    });

    return [stateChainWsConnection, lpWsConnection];
};

// Main application
const main = async () => {
    // await depositUsdcLiquidity('10000');
    await cancelAllOrders();
    const [stateChainWsConnection, lpWsConnection] = initializeLiquidityProviderBot();

    // Cleanup on process exit
    process.on('SIGINT', () => {
        logger.info('Received SIGINT, closing connection');
        stateChainWsConnection.complete();
        lpWsConnection.complete();
        process.exit();
    });
};

main();