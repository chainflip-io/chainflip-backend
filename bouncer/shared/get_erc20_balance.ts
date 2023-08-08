import Web3 from 'web3';
import { fineAmountToAmount } from './utils';
import { getErc20abi } from './eth_abis';

const erc20abi = await getErc20abi();

export async function getErc20Balance(
  walletAddress: string,
  contractAddress: string,
): Promise<string> {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contract = new web3.eth.Contract(erc20abi as any, contractAddress);

  const decimals = await contract.methods.decimals().call();
  const fineBalance: string = await contract.methods.balanceOf(walletAddress).call();
  return fineAmountToAmount(fineBalance, decimals);
}
