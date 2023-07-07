import { Asset } from "@chainflip-io/cli/.";
import { sendDot } from "./send_dot";
import { sendBtc } from "./send_btc";
import { sendUsdc } from "./send_usdc";
import { sendEth } from "./send_eth";
import { sendFlip } from "./send_flip";

export async function send(token: Asset, address: string, amount?: string) {
    if (token === 'BTC') {
        await sendBtc(address, amount ?? '0.05');
    } else if (token === 'ETH') {
        await sendEth(address, amount ?? '5');
    } else if (token === 'DOT') {
        await sendDot(address, amount ?? '50');
    } else if (token === 'USDC') {
        await sendUsdc(address, amount ?? '500');
    } else if (token === 'FLIP') {
        await sendFlip(address, amount ?? '10');
    }
}