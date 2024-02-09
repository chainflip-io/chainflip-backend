import { getTokenContractAddress } from '@/shared/contracts';
import {
  Asset,
  AssetAndChain,
  assetChains,
  assetDecimals,
  Assets,
  ChainflipNetwork,
  isTestnet,
} from '@/shared/enums';
import type { Environment } from '@/shared/rpc';
import { readAssetValue } from '@/shared/rpc/utils';
import type { AssetData } from './types';

type AssetFn = (
  network: ChainflipNetwork,
  env: Pick<Environment, 'swapping' | 'ingressEgress'>,
) => AssetData;

const assetName: Record<Asset, string> = {
  [Assets.ETH]: 'Ether',
  [Assets.USDC]: 'USDC',
  [Assets.FLIP]: 'FLIP',
  [Assets.DOT]: 'Polkadot',
  [Assets.BTC]: 'Bitcoin',
};

const assetFactory =
  (asset: Asset): AssetFn =>
  (network, env) => {
    const assetAndChain = { asset, chain: assetChains[asset] } as AssetAndChain;

    return {
      id: asset,
      chain: assetChains[asset],
      contractAddress: getTokenContractAddress(asset, network, false),
      decimals: assetDecimals[asset],
      name: assetName[asset],
      symbol: asset,
      chainflipId: asset,
      isMainnet: !isTestnet(network),
      minimumSwapAmount: readAssetValue(
        env.ingressEgress.minimumDepositAmounts,
        assetAndChain,
      ).toString(),
      maximumSwapAmount:
        readAssetValue(
          env.swapping.maximumSwapAmounts,
          assetAndChain,
        )?.toString() ?? null,
      minimumEgressAmount: readAssetValue(
        env.ingressEgress.minimumEgressAmounts,
        assetAndChain,
      ).toString(),
    } as AssetData;
  };

export const eth$ = assetFactory('ETH');
export const usdc$ = assetFactory('USDC');
export const flip$ = assetFactory('FLIP');
export const dot$ = assetFactory('DOT');
export const btc$ = assetFactory('BTC');
