import Web3 from "web3";
import Keyring from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { getNextEthNonce } from "./send_eth";
import { getEthContractAddress } from "./utils";
import erc20abi from '../../eth-contract-abis/IERC20.json';
import gatewayabi from '../../eth-contract-abis/perseverance-rc17/IStateChainGateway.json';
import { observeEvent, getChainflipApi, hexStringToBytesArray } from '../shared/utils';

export async function fundFlip(pubkey: string, flipAmount: string) {

    const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
    const chainflip = await getChainflipApi();
    await cryptoWaitReady();
    const keyring = new Keyring();

    let flipperinoAmount;
    if (!flipAmount.includes('.')) {
        flipperinoAmount = flipAmount + '000000000000000000';
    } else {
        const amountParts = flipAmount.split('.');
        flipperinoAmount = amountParts[0] + amountParts[1].padEnd(18, '0').substr(0, 18);
    }

    const web3 = new Web3(ethEndpoint);

    const flipContractAddress =
        process.env.ETH_FLIP_ADDRESS ?? getEthContractAddress('FLIP');

    const gatewayContractAddress = 
        process.env.ETH_GATEWAY_ADDRESS ?? getEthContractAddress('GATEWAY');

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const flipContract = new web3.eth.Contract(erc20abi as any, flipContractAddress);
    const gatewayContract = new web3.eth.Contract(gatewayabi as any, gatewayContractAddress);

    var txData = flipContract.methods.approve(gatewayContractAddress, flipperinoAmount).encodeABI();
    const whaleKey = process.env.ETH_USDC_WHALE || '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
    console.log('Approving ' + flipAmount + ' FLIP to State Chain Gateway');

    var nonce = await getNextEthNonce()
    var tx = { to: flipContractAddress, data: txData, gas: 2000000, nonce };
    var signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
    var receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string, ((error, hash) => {
        if (error) {
            console.error("Ethereum transaction failure:", error);
        }
    }));
    console.log("Transaction complete, tx_hash: " + receipt.transactionHash + " blockNumber: " + receipt.blockNumber + " blockHash: " + receipt.blockHash);
    
    txData = gatewayContract.methods.fundStateChainAccount(pubkey, flipperinoAmount).encodeABI();
    console.log('Funding ' + flipAmount + ' FLIP to ' + pubkey);

    nonce = await getNextEthNonce()
    tx = { to: gatewayContractAddress, data: txData, gas: 2000000, nonce };
    signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
    receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string, ((error, hash) => {
        if (error) {
            console.error("Ethereum transaction failure:", error);
        }
    }));
    console.log("Transaction complete, tx_hash: " + receipt.transactionHash + " blockNumber: " + receipt.blockNumber + " blockHash: " + receipt.blockHash);
	await observeEvent('funding:Funded', chainflip, (data) => {
		return Array.from(keyring.decodeAddress(data[0])).toString() == hexStringToBytesArray(pubkey).toString();
	});
}