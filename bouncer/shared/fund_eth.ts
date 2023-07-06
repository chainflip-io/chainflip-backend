import { Mutex } from "async-mutex";
import Web3 from "web3";

let nextNonce: number | undefined;

const mutex = new Mutex();

export async function getNextEthNonce(): Promise<number> {
    return mutex.runExclusive(async () => {
        if (nextNonce === undefined) {
            const ethEndpoint = process.env.ETH_ENDPOINT || "http://127.0.0.1:8545";
            const web3 = new Web3(ethEndpoint);
            const whaleKey = process.env.ETH_USDC_WHALE || '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
            const address = web3.eth.accounts.privateKeyToAccount(whaleKey).address;
            const txCount = await web3.eth.getTransactionCount(address);
            nextNonce = txCount;
        }
        return nextNonce++;
    });
}

export async function fundEth(ethereumAddress: string, ethAmount: string) {

    const ethEndpoint = process.env.ETH_ENDPOINT || "http://127.0.0.1:8545";
    const web3 = new Web3(ethEndpoint);

    let weiAmount;
    if (ethAmount.indexOf('.') === -1) {
        weiAmount = ethAmount + "000000000000000000";
    } else {
        const amountParts = ethAmount.split('.');
        weiAmount = amountParts[0] + amountParts[1].padEnd(18, '0').substring(0, 18);
    }

    const whaleKey = process.env.ETH_USDC_WHALE || '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';

    const nonce = await getNextEthNonce();

    const tx = {
        to: ethereumAddress,
        value: weiAmount,
        gas: 2000000,
        nonce,
    };

    const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
    let receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string, ((error, hash) => {
        if (error) {
            console.error("Eth transaction failure:", error);
        }
    }));

    console.log("Transaction complete, tx_hash: " + receipt.transactionHash + " blockNumber: " + receipt.blockNumber + " blockHash: " + receipt.blockHash);
}