#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted as USDC
//
// For example: ./commands/send_arbusdc.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 ARBUSDC to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import Web3 from 'web3';
import { Wallet, getDefaultProvider } from 'ethers';
import { runWithTimeout, getEvmContractAddress, getEvmEndpoint } from '../shared/utils';
import { sendErc20 } from '../shared/send_erc20';

async function main(): Promise<void> {
  const arbitrumAddress = process.argv[2];
  const arbusdcAmount = process.argv[3].trim();

  // Debugging
  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));

  // This has sent 24 transactions (probably sequencer stuff)
  const arbSeed = 'indoor dish desk flag debris potato excuse depart ticket judge file exit';

  const wallet = Wallet.fromPhrase(arbSeed).connect(getDefaultProvider(getEvmEndpoint('Arbitrum')));
  console.log(wallet.address);

  // console.log('txsSent', await arbClient.eth.getTransactionCount(wallet.address));

  // This has sent 0 transactions
  // console.log(
  //   'txsSent',
  //   await arbClient.eth.getTransactionCount('0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266'),
  // );
  // console.log('VAULT', await arbClient.eth.getCode('0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512'));
  // console.log(
  //   'KEY_MANAGER',
  //   await arbClient.eth.getCode('0x5FbDB2315678afecb367f032d93F642f64180aa3'),
  // );
  // console.log(
  //   'ADDRESS_CHECKER',
  //   await arbClient.eth.getCode('0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0'),
  // );
  // console.log('ARBUSDC', await arbClient.eth.getCode('0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9'));

  const contractAddress = getEvmContractAddress('Arbitrum', 'ARBUSDC');
  console.log('contractAddress: ', contractAddress);
  await sendErc20('Arbitrum', arbitrumAddress, contractAddress, arbusdcAmount);

  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
