import Web3 from 'web3';
import { assetDecimals } from '@chainflip-io/cli';
import { fineAmountToAmount } from './utils';

export async function getEthBalance(address: string): Promise<string> {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';

  const web3 = new Web3(ethEndpoint);

  const weiBalance: string = await web3.eth.getBalance(address);
  return fineAmountToAmount(weiBalance, assetDecimals.ETH);
}
