// INSTRUCTIONS
//
// This command takes one argument
// It will take the provided seed turn it into a new ethereum address and return the address
// For example: pnpm tsx ./commands/new_eth_address.ts foobar
// returns: 0xE16CCFc63368e8FC93f53ccE4e4f4b08c4C3E186

import Web3 from 'web3';
import { sha256 } from '../shared/utils';

async function main(): Promise<void> {
  const seed = process.argv[2] ?? '';
  const secret = sha256(seed).toString('hex');
  const web3 = new Web3();
  console.log(web3.eth.accounts.privateKeyToAccount(secret).address);
}

await main();
