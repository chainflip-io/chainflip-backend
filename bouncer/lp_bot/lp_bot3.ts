#!/usr/bin/env -S pnpm tsx
import { webSocket, WebSocketSubject } from 'rxjs/webSocket';
import { Subject, Observable, from, merge } from 'rxjs';
import { filter, map, mergeMap } from 'rxjs/operators';
import WebSocket from 'ws';
import { lpApiRpc } from '../shared/json_rpc';
import { amountToFineAmount, Asset, assetDecimals, chainFromAsset } from '../shared/utils';
import { stateChainAssetFromAsset } from '../shared/utils';
import { globalLogger as logger } from '../shared/utils/logger';

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

type Price = {
    fromAsset: { chain: string; asset: string };
    toAsset: { chain: string; asset: string };
    price: number;
    sqrtPrice: number;
    tick: number;
}

type TradeDecision = {
    shouldTrade: boolean;
    side: Side;
    asset: Asset;
    amount: number;
    price: number;
}

// Pure functions for decision making
const analyzeSwap = (swap: Swap): TradeDecision => {
    return {
        shouldTrade: true,
        side: swap.side === Side.Sell ? Side.Buy : Side.Sell, // Take opposite side
        asset: swap.baseAsset,
        amount: swap.amount,
        price: 0 // TODO: Add price calculation
    };
};

// Pure function for order creation
const createOrderPayload = (decision: TradeDecision) => {
    const orderId = Math.floor(Math.random() * 10000) + 1;
    const buyOrSellAmount = decision.side === Side.Buy
        ? parseInt(amountToFineAmount(decision.amount.toString(), assetDecimals('Usdc')))
        : parseInt(amountToFineAmount(decision.amount.toString(), assetDecimals(decision.asset)));

    return {
        orderId,
        params: [
            decision.asset,
            { chain: 'Ethereum', asset: 'USDC' },
            decision.side,
            orderId,
            0,
            buyOrSellAmount,
        ]
    };
};

// Side effects are isolated in these functions
const executeOrder = async (orderPayload: ReturnType<typeof createOrderPayload>) => {
    // try {
    //     const response = await lpApiRpc(logger, 'lp_set_limit_order', orderPayload.params);
    //     console.log(`Order executed: ${orderPayload.orderId}, Response: ${response}`);
    //     return orderPayload.orderId;
    // } catch (error) {
    //     console.error(`Failed to execute order: ${error}`);
    //     return null;
    // }
    return orderPayload.orderId;
};

// Transform the WebSocket stream into a stream of MarketEvents
const createSwapStream = (wsConnection: any): Observable<Swap> => {
    return wsConnection.pipe(
        filter((msg: any) => msg.method === 'cf_subscribe_scheduled_swaps'),
        map((msg: any) => msg.params.result.swaps),
        mergeMap((swaps: any[]) => from(swaps)),
        map((swap: any): Swap => ({
            baseAsset: swap.base_asset,
            quoteAsset: { chain: 'Ethereum', asset: 'USDC' },
            side: swap.side,
            amount: swap.amount
        }))
    );
};

const createPriceStream = (wsConnection: any, fromAsset: { chain: string, asset: string }, toAsset: { chain: string, asset: string }): Observable<Price> => {
    return wsConnection.pipe(
        filter((msg: any) => msg.method === 'cf_subscribe_pool_price'),
        map((msg: any): Price => ({
            fromAsset,
            toAsset,
            sqrtPrice: msg.params.result.sqrt_price,
            tick: msg.params.result.tick,
            price: msg.params.result.price
        }))
    );
};

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

// WebSocket setup and subscription handling
const initializeLiquidityProviderBot = () => {
    const wsConnection = webSocket('ws://127.0.0.1:9944');

    const subscribeToAsset = (chain: string, asset: string) => ({
        id: 1,
        jsonrpc: "2.0",
        method: "cf_subscribe_scheduled_swaps",
        params: {
            base_asset: { chain, asset },
            quote_asset: { chain: "Ethereum", asset: "USDC" }
        }
    });

    // Create our reactive pipeline
    const swaps$ = createSwapStream(wsConnection);
    const tradeDecisions$ = createTradeDecisionStream(swaps$);
    const orders$ = createOrderStream(tradeDecisions$);

    // Subscribe to the final stream to start the flow
    orders$.subscribe({
        next: (orderId) => {
            if (orderId) {
                console.log(`Order executed successfully: ${orderId}`);
            }
        },
        error: (err) => console.error('Error in order stream:', err),
        complete: () => console.log('Order stream completed')
    });

    // Initialize subscriptions
    wsConnection.next(subscribeToAsset('Bitcoin', 'BTC'));
    wsConnection.next(subscribeToAsset('Ethereum', 'ETH'));

    return wsConnection;
};

// Main application
const main = () => {
    const connection = initializeLiquidityProviderBot();

    // Cleanup on process exit
    process.on('SIGINT', () => {
        connection.complete();
        process.exit();
    });
};

main();