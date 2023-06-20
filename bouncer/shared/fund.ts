import { fundDot } from "./fund_dot";
import { Token } from "./utils";
import { fundBtc } from "./fund_btc";
import { fundUsdc } from "./fund_usdc";
import { fundEth } from "./fund_eth";

export async function fund(token: Token, address: string) {
    if (token === 'BTC') {
        await fundBtc(address, '0.5');
    } else if (token === 'ETH') {
        await fundEth(address, '5');
    } else if (token === 'DOT') {
        await fundDot(address, '50');
    } else if (token === 'USDC') {
        await fundUsdc(address, '500');
    }
}