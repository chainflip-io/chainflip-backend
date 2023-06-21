// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount of
// tokens provided in the second argument. The token amount is interpreted as USDC
//
// For example: pnpm tsx ./commands/fund_usdc.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 USDC to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import Web3 from 'web3';
import { runWithTimeout } from '../shared/utils';

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

async function main(): Promise<void> {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const ethereumAddress = process.argv[2];
  const usdcAmount = process.argv[3].trim();

  console.log("Submitting transaction to transfer USDC to " + ethereumAddress + " for " + usdcAmount + " USDC")
  let microusdcAmount;
  if (!usdcAmount.includes('.')) {
    microusdcAmount = usdcAmount + '000000';
  } else {
    const amountParts = usdcAmount.split('.');
    microusdcAmount = amountParts[0] + amountParts[1].padEnd(6, '0').substr(0, 6);
  }
  const web3 = new Web3(ethEndpoint);
  // const chainid = await web3.eth.getChainId();
  const usdcContractAddress =
    process.env.ETH_USDC_ADDRESS ?? '0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0';
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const usdcContract = new web3.eth.Contract(erc20TransferABI as any, usdcContractAddress);
  const txData = usdcContract.methods.transfer(ethereumAddress, microusdcAmount).encodeABI();
  const whaleKey = process.env.ETH_USDC_WHALE || '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
  console.log('Transferring ' + usdcAmount + ' USDC to ' + ethereumAddress);
  const tx = { to: usdcContractAddress, data: txData, gas: 2000000 };
  const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
  await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);
  console.log("Transfer done");
  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
