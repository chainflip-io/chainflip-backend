import { Asset } from '@chainflip-io/cli';
import { getEthContractAddress } from './utils';
import { getBtcBalance } from './get_btc_balance';
import { getDotBalance } from './get_dot_balance';
import { getEthBalance } from './get_eth_balance';
import { getErc20Balance } from './get_erc20_balance';

export async function getBalance(token: Asset, address: string): Promise<string> {
  // eslint-disable-next-line no-param-reassign
  address = address.trim();
  let result: string;
  switch (token) {
    case 'FLIP':
    case 'USDC':
      const contractAddress = getEthContractAddress(token);
      result = await getErc20Balance(address, contractAddress);
      break;
    case 'ETH':
      result = await getEthBalance(address);
      break;
    case 'DOT':
      result = await getDotBalance(address);
      break;
    case 'BTC':
      result = (await getBtcBalance(address)).toString().trim();
      break;
    default:
      throw new Error(`Unexpected token: ${token}`);
  }
  return result;
}
