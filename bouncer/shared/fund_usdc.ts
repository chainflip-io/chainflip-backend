import { Mutex } from "async-mutex";
import Web3 from "web3";
import { getNextEthNonce } from "./fund_eth";
import { getEthContractAddress } from "./utils";

const erc20TransferABI = [
    // transfer
    {
        constant: false,
        inputs: [
            {
                name: '_to',
                type: 'address',
            },
            {
                name: '_value',
                type: 'uint256',
            },
        ],
        name: 'transfer',
        outputs: [
            {
                name: '',
                type: 'bool',
            },
        ],
        type: 'function',
    },
];

export async function fundUsdc(ethereumAddress: string, usdcAmount: string) {

    const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';

    let microusdcAmount;
    if (!usdcAmount.includes('.')) {
        microusdcAmount = usdcAmount + '000000';
    } else {
        const amountParts = usdcAmount.split('.');
        microusdcAmount = amountParts[0] + amountParts[1].padEnd(6, '0').substr(0, 6);
    }

    const web3 = new Web3(ethEndpoint);

    const usdcContractAddress =
        process.env.ETH_USDC_ADDRESS ?? getEthContractAddress('USDC');

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const usdcContract = new web3.eth.Contract(erc20TransferABI as any, usdcContractAddress);
    const txData = usdcContract.methods.transfer(ethereumAddress, microusdcAmount).encodeABI();
    const whaleKey = process.env.ETH_USDC_WHALE || '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
    console.log('Transferring ' + usdcAmount + ' USDC to ' + ethereumAddress);

    const nonce = await getNextEthNonce()
    const tx = { to: usdcContractAddress, data: txData, gas: 2000000, nonce };

    const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
    await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);
}