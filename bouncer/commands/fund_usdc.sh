#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount of
// tokens provided in the second argument. The token amount is interpreted as USDC
//
// For example: ./commands/fund_usdc.sh 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 USDC to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

const Web3 = require('web3');
const { runWithTimeout } = require('../shared/utils');

const erc20TransferABI = [
  // transfer
  {
    "constant": false,
    "inputs": [
      {
        "name": "_to",
        "type": "address"
      },
      {
        "name": "_value",
        "type": "uint256"
      }
    ],
    "name": "transfer",
    "outputs": [
      {
        "name": "",
        "type": "bool"
      }
    ],
    "type": "function"
  }
];

async function main() {
  const eth_endpoint = process.env.ETH_ENDPOINT || 'http://127.0.0.1:8545';
  const ethereum_address = process.argv[2];
  const usdc_amount = process.argv[3].trim();
  var microusdc_amount;
  if(usdc_amount.indexOf('.') == -1){
    microusdc_amount = usdc_amount + "000000";
  } else {
    const amount_parts = usdc_amount.split('.');
    microusdc_amount = amount_parts[0] + amount_parts[1].padEnd(6,'0').substr(0, 6);
  }
	const web3 = new Web3(eth_endpoint);
	const chainid = await web3.eth.getChainId();
	const usdcContractAddress = process.env.ETH_USDC_ADDRESS || '0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512';
	const usdcContract = new web3.eth.Contract(erc20TransferABI, usdcContractAddress);
	const txData = usdcContract.methods.transfer(ethereum_address, microusdc_amount).encodeABI();
	const whaleKey = process.env.ETH_USDC_WHALE || '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
  console.log("Transferring " + usdc_amount + " USDC to " + ethereum_address);
	const tx = {to: usdcContractAddress,
				data: txData,
				gas: 2000000};
	const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
	let receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction);
  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
	console.error(error);
	process.exit(-1);
});
