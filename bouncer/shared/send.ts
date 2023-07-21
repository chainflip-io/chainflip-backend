import { Asset } from '@chainflip-io/cli';
import { sendDot } from './send_dot';
import { sendBtc } from './send_btc';
import { sendErc20 } from './send_erc20';
import { sendEth } from './send_eth';
import { getEthContractAddress, defaultAssetAmounts } from './utils';

export async function send(asset: Asset, address: string, amount?: string) {
  switch (asset) {
    case 'BTC':
      await sendBtc(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'ETH':
      await sendEth(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'DOT':
      await sendDot(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'USDC':
    case 'FLIP': {
      const contractAddress = getEthContractAddress(asset);
      await sendErc20(address, contractAddress, amount ?? defaultAssetAmounts(asset));
      break;
    }
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }
}
