#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted as USDC
//
// For example: ./commands/get_address_bytecode.ts 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9

import Web3 from 'web3';
import { runWithTimeout, getEvmEndpoint } from '../shared/utils';

async function main(): Promise<void> {
  // const arbitrumAddress = process.argv[2];
  const arbitrumAddress = '0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9';

  const web3 = new Web3(getEvmEndpoint('Arbitrum'));
  console.log(
    `Address: ${arbitrumAddress} has bytecode ${await web3.eth.getCode(arbitrumAddress)}`,
  );

  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
