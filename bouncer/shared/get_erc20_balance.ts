import Web3 from "web3";
import erc20abi from '../../eth-contract-abis/IERC20.json';

export async function getErc20Balance(walletAddress: string, contractAddress: string): Promise<string> {

    const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
    const web3 = new Web3(ethEndpoint);
    
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const contract = new web3.eth.Contract(erc20abi as any, contractAddress);

    const decimals = await contract.methods.decimals().call();
    const fineBalance: string = await contract.methods.balanceOf(walletAddress).call();
    const balanceLen = fineBalance.length;
    let balance;
    if (balanceLen > decimals) {
        const decimalLocation = balanceLen - decimals;
        balance = fineBalance.slice(0, decimalLocation) + '.' + fineBalance.slice(decimalLocation);
    } else {
        balance = '0.' + fineBalance.padStart(decimals, '0');
    }

    return balance;
}