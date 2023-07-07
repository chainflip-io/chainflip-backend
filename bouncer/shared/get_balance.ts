import { Token, getEthContractAddress } from "./utils";
import { getBtcBalance } from "./get_btc_balance";
import { getDotBalance } from "./get_dot_balance";
import { getEthBalance } from "./get_eth_balance";
import { getErc20Balance } from "./get_erc20_balance";

export type Token = 'USDC' | 'ETH' | 'DOT' | 'FLIP' | 'BTC';

export async function getBalance(token: Token, address: string): Promise<number> {
    address = address.trim();
    let result: any;
    switch (token) {
        case 'FLIP':
        case 'USDC':
            const contractAddress = getEthContractAddress(token);
            result = await getErc20Balance(address, contractAddress);
            break;
        case 'ETH':
            result = await getEthBalance(address);
            break;
        case 'DOT':
            result = await getDotBalance(address);
            break;
        case "BTC":
            result = await getBtcBalance(address);
            break;
        default:
            throw new Error(`Unexpected token: ${token}`);
    }
    return result.toString().trim();
}