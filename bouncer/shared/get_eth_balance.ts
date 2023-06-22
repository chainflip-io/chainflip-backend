import Web3 from "web3";

export async function getEthBalance(address: string): Promise<string> {

    const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';

    const web3 = new Web3(ethEndpoint);

    const weiBalance: string = await web3.eth.getBalance(address);
    const balanceLen = weiBalance.length;
    let balance;
    if (balanceLen > 18) {
        const decimalLocation = balanceLen - 18;
        balance = weiBalance.slice(0, decimalLocation) + '.' + weiBalance.slice(decimalLocation);
    } else {
        balance = '0.' + weiBalance.padStart(18, '0');
    }

    return balance
}