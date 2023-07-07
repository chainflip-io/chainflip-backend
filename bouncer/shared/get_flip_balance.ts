import Web3 from "web3";
import { getEthContractAddress } from "./utils";
import erc20abi from '../../eth-contract-abis/IERC20.json';

export async function getFlipBalance(ethereumAddress: string): Promise<string> {

    const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
    const web3 = new Web3(ethEndpoint);
    const flipContractAddress =
        process.env.ETH_FLIP_ADDRESS ?? getEthContractAddress('FLIP');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const flipContract = new web3.eth.Contract(erc20abi as any, flipContractAddress);

    const rawBalance: string = await flipContract.methods.balanceOf(ethereumAddress).call();
    const balanceLen = rawBalance.length;
    let balance;
    if (balanceLen > 18) {
        const decimalLocation = balanceLen - 18;
        balance = rawBalance.slice(0, decimalLocation) + '.' + rawBalance.slice(decimalLocation);
    } else {
        balance = '0.' + rawBalance.padStart(18, '0');
    }

    return balance;
}