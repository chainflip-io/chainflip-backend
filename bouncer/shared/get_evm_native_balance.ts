import { Chain, assetDecimals } from '@chainflip-io/cli';
import Web3 from 'web3';
import { fineAmountToAmount } from './utils';

export async function getEvmNativeBalance(chain: Chain, address: string): Promise<string> {
  const evmEndpoint =
    chain === 'Ethereum'
      ? process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'
      : process.env.ARB_ENDPOINT ?? 'http://127.0.0.1:8547';

  const web3 = new Web3(evmEndpoint);

  const weiBalance: string = await web3.eth.getBalance(address);
  return fineAmountToAmount(weiBalance, assetDecimals.ETH);
}
