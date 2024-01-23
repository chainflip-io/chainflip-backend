import { Asset, Chains } from '@chainflip-io/cli';
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
      const contractAddress = getEvmContractAddress(Chains.Ethereum, asset);
      result = await getErc20Balance(Chains.Ethereum, address, contractAddress);
      break;
    }
    case 'ARBUSDC': {
      const contractAddress = getEvmContractAddress(Chains.Arbitrum, asset);
      result = await getErc20Balance(Chains.Arbitrum, address, contractAddress);
      break;
    }
    case 'ETH':
      result = await getEvmNativeBalance(Chains.Ethereum, address);
      break;
    case 'ARB':
      result = await getEvmNativeBalance(Chains.Arbitrum, address);
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
