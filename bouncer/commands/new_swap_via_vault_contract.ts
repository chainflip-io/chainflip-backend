// Swaps ETH to DOT via the Vault contract by submitting a transaction to the Vault contract
// with ETH.

import Web3 from 'web3';
import fs from 'fs';
import { runWithTimeout } from '../shared/utils';
import assert from 'assert';
import { decodeAddress } from '@polkadot/util-crypto';

function polkadotAddressToHex(address: string): string {
  // Decode the address
  const rawBytes = decodeAddress(address);

  // Convert to hexadecimal string
  const hexString = '0x' + Buffer.from(rawBytes).toString('hex');

  assert(hexString.length === 66);

  return hexString;
}
async function main(): Promise<string> {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  const dstAddress = process.argv[2];
  const ethAmount = process.argv[3].trim();

  console.log('Got eth amount: {}', ethAmount);

  const dstAddressBytes = polkadotAddressToHex(dstAddress);

  // set the contract address and ABI
  const contractAddress = '0xb7a5bd0345ef1cc5e66bf61bdec17d2461fbd968';

  const vaultJSONString = fs.readFileSync('./cf-abis/IVault.json', 'utf8');
  const contractABI = JSON.parse(vaultJSONString);

  // create a new contract instance
  const contract = new web3.eth.Contract(contractABI, contractAddress);

  // Some arbitrary cfParameters
  const cfParameters = '0x123741231';

  // Swap from ETH to Dot (Asset 4) on Polkadot chain (ForeignChain 2)
  const xSwapNative = contract.methods.xSwapNative(2, dstAddressBytes, 4, cfParameters);

  const tx = {
    from: '0x1234567890123456789012345678901234567890',
    to: contractAddress,
    gas: 200000,
    data: xSwapNative.encodeABI(),
    value: ethAmount,
  };

  const whaleKey = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';

  const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
  const txReceipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);
  console.log('Transaction hash:', txReceipt.transactionHash);
  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
