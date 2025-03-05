#!/usr/bin/env -S pnpm tsx
import { webSocket } from 'rxjs/webSocket';
import WebSocket from 'ws';
import { lpApiRpc } from '../shared/json_rpc';
import { amountToFineAmount, Asset, assetDecimals, chainFromAsset } from '../shared/utils';
import { stateChainAssetFromAsset } from '../shared/utils';
import { globalLogger as logger } from '../shared/utils/logger';

// Have to do this because rxjs/webSocket uses the global.WebSocket
(global as any).WebSocket = WebSocket;

enum Side {
    Buy = 'buy',
    Sell = 'sell'
}

const btc = {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "cf_subscribe_scheduled_swaps",
    "params": {
        "base_asset": {
            "chain": "Bitcoin",
            "asset": "BTC"
        },
        "quote_asset": {
            "chain": "Ethereum",
            "asset": "USDC"
        }
    }
};

const eth = {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "cf_subscribe_scheduled_swaps",
    "params": {
        "base_asset": {
            "chain": "Ethereum",
            "asset": "ETH"
        },
        "quote_asset": {
            "chain": "Ethereum",
            "asset": "USDC"
        }
    }
};

const orderBook = [];

async function deployLiquidity() { }

async function buyOrder() {

}

function generateRandomOrderId(): number {
    return Math.floor(Math.random() * 10000) + 1;
}


async function openLimitOrder(chain: Chain, asset: Asset, side: Side, price: number, amount: number): Promise<number> {
    console.log(`Opening limit order for ${asset} ${side} ${price} ${amount}`);
    let orderId = generateRandomOrderId();

    let tick = Math.round(Math.log(Math.sqrt(price)) / Math.log(Math.sqrt(1.0001)));

    let buyOrSellAmount = 0;

    if (side === Side.Buy) {
        // For buy orders meassured in quote asset
        buyOrSellAmount = parseInt(
            amountToFineAmount(amount.toString(), assetDecimals('Usdc')),
        );
    } else {
        // For sell orders meassured in quote base
        buyOrSellAmount = buyOrSellAmount = parseInt(
            amountToFineAmount(amount.toString(), assetDecimals(asset)),
        );
    }

    console.log(`Order ID: ${orderId}, amount: ${buyOrSellAmount}, side: ${side}, price: ${price}`);

    try {
        let response = await lpApiRpc(logger, 'lp_set_limit_order',
            [
                // Base Asset
                {
                    chain,
                    asset,
                },
                // Quote Asset
                {
                    chain: 'Ethereum',
                    asset: 'USDC',
                },
                side,
                orderId,
                0,
                buyOrSellAmount,
            ],
        );
        console.log(`Response: ${response}`);
    } catch (error) {
        console.error(`Error opening limit order: ${error}`);
    }

    return orderId;
}

async function closeLimitOrder(orderId: number) {
    // TODO: Implemet this
}

async function isProftiable(swap: any): Promise<boolean> {
    // TODO: Implement this
    return true;
}

async function processSwap(swaps: any) {
    for (const swap of swaps) {
        // If someone sells we want to buy
        if (swap.side == Side.Sell) {
            let orderId = await openLimitOrder(swap.base_asset.chain, swap.base_asset.asset, Side.Buy, 0, swap.amount);
            orderBook.push(orderId);
        } else {
            // If someone buys we want to sell
            let orderId = await openLimitOrder(swap.base_asset.chain, swap.base_asset.asset, Side.Sell, 0, swap.amount);
            orderBook.push(orderId);
        }
    }
}

async function main() {
    const stateChainSubscription = webSocket('ws://127.0.0.1:9944');

    stateChainSubscription.subscribe({
        next: (msg: any) => {
            if (msg.method === 'cf_subscribe_scheduled_swaps') {
                console.log(`New swaps incoming 💸`);
                processSwap(msg.params.result.swaps);
            }
        },
        error: (err) => console.error('WebSocket error:', err),
        complete: () => console.log('WebSocket closed')
    });

    stateChainSubscription.next(btc);
    stateChainSubscription.next(eth);
}

main();
