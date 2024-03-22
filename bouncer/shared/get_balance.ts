import { InternalAsset as Asset } from '@chainflip/cli';
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
    case 'Flip':
    case 'Usdc': {
      const contractAddress = getEvmContractAddress('Ethereum', asset);
      result = await getErc20Balance('Ethereum', address, contractAddress);
      break;
    }
    case 'Usdt': {
      const contractAddress = getEvmContractAddress('Ethereum', asset);
      result = await getErc20Balance('Ethereum', address, contractAddress);
      break;
    }
    case 'ArbUsdc': {
      const contractAddress = getEvmContractAddress('Arbitrum', asset);
      result = await getErc20Balance('Arbitrum', address, contractAddress);
      break;
    }
    case 'Eth':
      result = await getEvmNativeBalance('Ethereum', address);
      break;
    case 'ArbEth':
      result = await getEvmNativeBalance('Arbitrum', address);
      break;
    case 'Dot':
      result = await getDotBalance(address);
      break;
    case 'Btc':
      result = (await getBtcBalance(address)).toString().trim();
      break;
    default:
      throw new Error(`Unexpected asset: ${asset}`);
  }
  return result;
}
