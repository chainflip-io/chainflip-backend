#!/usr/bin/env -S pnpm tsx
import { webSocket } from 'rxjs/webSocket';
import WebSocket from 'ws';

// Have to do this because rxjs/webSocket uses the global.WebSocket
(global as any).WebSocket = WebSocket;

// Subscription to all swaps on ethereum from ETH to USDC
const swapTopic = {
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

async function main() {
    const subject = webSocket('ws://127.0.0.1:9944');

    subject.subscribe({
        next: (msg) => console.log('Received message:', msg),
        error: (err) => console.error('WebSocket error:', err),
        complete: () => console.log('WebSocket closed')
    });

    subject.next(swapTopic);
}

main();
