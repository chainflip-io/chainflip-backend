import { Asset } from '@chainflip/cli';
import { getEvmContractAddress } from './utils';
import { getBtcBalance } from './get_btc_balance';
import { getDotBalance } from './get_dot_balance';
import { getEvmNativeBalance } from './get_evm_native_balance';
import { getErc20Balance } from './get_erc20_balance';

export async function getBalance(asset: Asset, address: string): Promise<string> {
  // eslint-disable-next-line no-param-reassign
  address = address.trim();
  let result: string;
  switch (asset) {
    case 'FLIP':
    case 'USDC': {
      const contractAddress = getEvmContractAddress('Ethereum', asset);
      result = await getErc20Balance('Ethereum', address, contractAddress);
      break;
    }
    case 'USDT': {
      const contractAddress = getEvmContractAddress('Ethereum', asset);
      result = await getErc20Balance('Ethereum', address, contractAddress);
      break;
    }
    case 'ARBUSDC': {
      const contractAddress = getEvmContractAddress('Arbitrum', asset);
      result = await getErc20Balance('Arbitrum', address, contractAddress);
      break;
    }
    case 'ETH':
      result = await getEvmNativeBalance('Ethereum', address);
      break;
    case 'ARBETH':
      result = await getEvmNativeBalance('Arbitrum', address);
      break;
    case 'DOT':
      result = await getDotBalance(address);
      break;
    case 'BTC':
      result = (await getBtcBalance(address)).toString().trim();
      break;
    default:
      throw new Error(`Unexpected asset: ${asset}`);
  }
  return result;
}
