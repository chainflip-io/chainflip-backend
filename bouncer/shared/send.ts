import { Asset } from '@chainflip-io/cli';
import { sendDot } from './send_dot';
import { sendBtc } from './send_btc';
import { sendErc20 } from './send_erc20';
import { sendEth } from './send_eth';
import { getEthContractAddress, defaultAssetAmounts } from './utils';

export async function send(token: Asset, address: string, amount?: string) {
  switch (token) {
    case 'BTC':
      await sendBtc(address, amount ?? defaultAssetAmounts(token));
      break;
    case 'ETH':
      await sendEth(address, amount ?? defaultAssetAmounts(token));
      break;
    case 'DOT':
      await sendDot(address, amount ?? defaultAssetAmounts(token));
      break;
    case 'USDC':
    case 'FLIP': {
      const contractAddress = getEthContractAddress(token);
      await sendErc20(address, contractAddress, amount ?? defaultAssetAmounts(token));
      break;
    }
    default:
      throw new Error(`Unsupported token type: ${token}`);
  }
}
