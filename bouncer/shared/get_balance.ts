import fs from 'fs/promises';
import { Asset } from "@chainflip-io/cli/.";
import { getBtcBalance } from "./get_btc_balance";
import { getDotBalance } from "./get_dot_balance";
import { getEthBalance } from "./get_eth_balance";
import { getUsdcBalance } from "./get_usdc_balance";
import { BigNumber, ethers } from "ethers";

export type Token = 'USDC' | 'ETH' | 'DOT' | 'FLIP' | 'BTC';

const erc20abi = await fs.readFile('../eth-contract-abis/IERC20.json', 'utf-8');

export async function getFlipBalance(address: string): Promise<BigNumber> {
    const flipContractAddress = "10C6E9530F1C1AF873a391030a1D9E8ed0630D26".toLowerCase();
    const provider = ethers.getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');

    const flipContract = new ethers.Contract(flipContractAddress, erc20abi, provider);

    return flipContract.balanceOf(address);
}

export async function getBalance(token: Asset, address: string): Promise<number> {
    address = address.trim();
    let result: any;
    switch (token) {
        case 'USDC':
            result = await getUsdcBalance(address);
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
        case 'FLIP':
            result = await getFlipBalance(address);
            break;
        default:
            throw new Error(`Unexpected token: ${token}`);
    }
    return result.toString().trim();
}