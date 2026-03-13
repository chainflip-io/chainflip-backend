import { Asset, getContractAddress } from 'shared/utils';
import { getBtcBalance } from 'shared/get_btc_balance';
import { getEvmNativeBalance } from 'shared/get_evm_native_balance';
import { getErc20Balance } from 'shared/get_erc20_balance';
import { getSolBalance } from 'shared/get_sol_balance';
import { getSolUsdcBalance } from 'shared/get_solusdc_balance';
import { getSolUsdtBalance } from 'shared/get_solusdt_balance';
import { getHubAssetBalance, getHubDotBalance } from 'shared/get_hub_balance';
import { getTrxBalance } from 'shared/get_trx_balance';
import { getTrc20Balance } from 'shared/get_trc20_balance';

export async function getBalance(asset: Asset, address: string): Promise<string> {
  // eslint-disable-next-line no-param-reassign
  address = address.trim();
  let result: string;
  switch (asset) {
    case 'Flip':
    case 'Usdc':
    case 'Usdt':
    case 'Wbtc': {
      const contractAddress = getContractAddress('Ethereum', asset);
      result = await getErc20Balance('Ethereum', address, contractAddress);
      break;
    }
    case 'ArbUsdc':
    case 'ArbUsdt': {
      const contractAddress = getContractAddress('Arbitrum', asset);
      result = await getErc20Balance('Arbitrum', address, contractAddress);
      break;
    }
    case 'Eth':
      result = await getEvmNativeBalance('Ethereum', address);
      break;
    case 'ArbEth':
      result = await getEvmNativeBalance('Arbitrum', address);
      break;
    case 'Btc':
      result = (await getBtcBalance(address)).toString().trim();
      break;
    case 'Sol':
      result = await getSolBalance(address);
      break;
    case 'SolUsdc':
      result = await getSolUsdcBalance(address);
      break;
    case 'SolUsdt':
      result = await getSolUsdtBalance(address);
      break;
    case 'HubDot':
      result = await getHubDotBalance(address);
      break;
    case 'HubUsdc':
    case 'HubUsdt':
      result = await getHubAssetBalance(asset, address);
      break;
    case 'Trx':
      result = await getTrxBalance(address);
      break;
    case 'TronUsdt': {
      const contractAddress = getContractAddress('Tron', 'TronUsdt');
      result = await getTrc20Balance(address, contractAddress);
      break;
    }
    default:
      throw new Error(`Unexpected asset: ${asset}`);
  }
  return result;
}
