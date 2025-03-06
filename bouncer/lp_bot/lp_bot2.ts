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

enum Side {
    Buy = 'buy',
    Sell = 'sell'
}

// Types for our domain
type Swap = {
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

const analyzeSwap = (swap: Swap): TradeDecision => {
    // logger.info(`Analyzing swap: ${JSON.stringify(swap)}`);

    // Only handle USDT pairs
    if (swap.baseAsset.asset !== 'USDT') {
        return { shouldTrade: false, side: swap.side, asset: swap.baseAsset, amount: 0, price: 0 };
    }
    // Place orders on both sides
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
    // const buyOrSellAmount = decision.side === Side.Buy
    //     ? parseInt(amountToFineAmount(decision.amount.toString(), assetDecimals('Usdc')))
    //     : parseInt(amountToFineAmount(decision.amount.toString(), assetDecimals(decision.asset)));

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

// Side effects are isolated in these functions
const executeOrder = async (orderPayload: ReturnType<typeof createOrderPayload>) => {
    console.log(`Executing order: ${JSON.stringify(orderPayload)}`);
    try {
        const response = await lpApiRpc(logger, 'lp_set_limit_order', orderPayload.params);
        // console.log(`Order executed: ${orderPayload.orderId}, Response: ${JSON.stringify(response)}`);
        return orderPayload.orderId;
    } catch (error) {
        console.error(`Failed to execute order: ${error}`);
        return null;
    }
};

// Transform the WebSocket stream into a stream of MarketEvents
const createSwapStream = (wsConnection: any): Observable<Swap> => {
    return wsConnection.pipe(
        filter((msg: any) => msg.method === 'cf_subscribe_scheduled_swaps'),
        map((msg: any) => msg.params.result.swaps),
        mergeMap((swaps: any[]) => from(swaps)),
        map((swap: any): Swap => ({
            baseAsset: swap.base_asset,
            quoteAsset: swap.quote_asset,
            side: swap.side,
            amount: swap.amount
        }))
    );
};

const createOrderFillStream = (wsConnection: any) => {
    wsConnection.pipe(
        filter((msg: any) => msg.method === 'lp_subscribe_order_fills'),
        map((msg: any) => console.log("Order fill Jan: ", msg)));
}

// Transform MarketEvents into TradeDecisions
const createTradeDecisionStream = (swaps$: Observable<Swap>): Observable<TradeDecision> => {
    return swaps$.pipe(
        map(analyzeSwap),
        filter(decision => decision.shouldTrade)
    );
};

// Transform TradeDecisions into order executions
const createOrderStream = (decisions$: Observable<TradeDecision>): Observable<number | null> => {
    return decisions$.pipe(
        map(createOrderPayload),
        mergeMap(orderPayload => from(executeOrder(orderPayload)))
    );
};

const depositUsdcLiquidity = async (amount: string) => {
    console.log(`Depositing ${amount} USDC`);
    const contractAddress = getContractAddress('Ethereum', 'Usdc');
    console.log(`Contract address: ${contractAddress}`);
    const liquidityDepositAddress = await lpApiRpc(logger, 'lp_liquidity_deposit', [{ chain: 'Ethereum', asset: 'USDC' }, 'InBlock']);
    console.log(`Address: ${liquidityDepositAddress.tx_details.response}`);
    await sendErc20(logger, 'Ethereum', liquidityDepositAddress.tx_details.response, contractAddress, amount);
    console.log(`Liquidity deposited: ${amount}`);
}

const cancelAllOrders = async () => {
    const response = await lpApiRpc(logger, 'lp_cancel_all_orders', []);
    console.log(`Cancel all orders response: ${JSON.stringify(response, null, 2)}`);
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
                logger.info(`Order executed successfully: ${orderId}`);
            }
        },
        error: (err) => logger.error('Error in order stream:', err),
        complete: () => logger.info('Order stream completed')
    });

    // Initialize subscriptions
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
            console.log(`${JSON.stringify(msg, null, 2)}`);
        }
    });

    return [stateChainWsConnection, lpWsConnection];
};

// Main application
const main = async () => {
    // logger.info('Starting liquidity provider bot');
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