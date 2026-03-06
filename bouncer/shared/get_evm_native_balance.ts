import { ChainflipChain as Chain } from '@chainflip/utils/chainflip';
import { assetDecimals, fineAmountToAmount, getWeb3 } from 'shared/utils';

export async function getEvmNativeBalance(chain: Chain, address: string): Promise<string> {
  const web3 = getWeb3(chain);

  const weiBalance: string = await web3.eth.getBalance(address);
  return fineAmountToAmount(weiBalance, assetDecimals('Eth'));
}
