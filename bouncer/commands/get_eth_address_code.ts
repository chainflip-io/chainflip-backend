#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print wether the ETH address has code

import { getEvmEndpoint, runWithTimeoutAndExit } from '../shared/utils';
import Web3 from 'web3';

export async function getEthAddressCode(address: string) {
  const web3 = new Web3(getEvmEndpoint("Ethereum"));
  try {
    // Get the bytecode at the address
    const bytecode = await web3.eth.getCode(address);
    
    // Check if the address has code (contracts have bytecode longer than just "0x")
    const isContract = bytecode !== '0x';
    
    // Print the results
    // console.log(`Address: ${address}`);
    // console.log(`Bytecode: ${bytecode}`);
    console.log(`Is Contract: ${isContract ? 'Yes' : 'No'}`);
    // console.log(`Bytecode Length: ${bytecode.length} bytes`);
    
    return isContract;
  } catch (error) {
    console.error(`Error fetching bytecode: ${error instanceof Error ? error.message : String(error)}`);
    throw error;
  }
}

const ethereumAddress = process.argv[2] ?? '0';
await runWithTimeoutAndExit(getEthAddressCode(ethereumAddress), 5);
