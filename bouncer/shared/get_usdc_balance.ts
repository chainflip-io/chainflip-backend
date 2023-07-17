import Web3 from 'web3';
import { getEthContractAddress } from './utils';
import erc20abi from '../../eth-contract-abis/IERC20.json';

export async function getUsdcBalance(ethereumAddress: string): Promise<string> {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);
  const usdcContractAddress = process.env.ETH_USDC_ADDRESS ?? getEthContractAddress('USDC');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const usdcContract = new web3.eth.Contract(erc20abi as any, usdcContractAddress);

  const rawBalance: string = await usdcContract.methods.balanceOf(ethereumAddress).call();
  const balanceLen = rawBalance.length;
  let balance;
  if (balanceLen > 6) {
    const decimalLocation = balanceLen - 6;
    balance = rawBalance.slice(0, decimalLocation) + '.' + rawBalance.slice(decimalLocation);
  } else {
    balance = '0.' + rawBalance.padStart(6, '0');
  }

  return balance;
}
