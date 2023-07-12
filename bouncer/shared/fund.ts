import { Asset } from "@chainflip-io/cli/.";
import { fundDot } from "./fund_dot";
import { fundBtc } from "./fund_btc";
import { fundUsdc } from "./fund_usdc";
import { fundEth } from "./fund_eth";

export async function fund(token: Asset, address: string, amount?: string) {
    if (token === 'BTC') {
        await fundBtc(address, amount ?? '0.05');
    } else if (token === 'ETH') {
        await fundEth(address, amount ?? '5');
    } else if (token === 'DOT') {
        await fundDot(address, amount ?? '50');
    } else if (token === 'USDC') {
        await fundUsdc(address, amount ?? '500');
    }
}