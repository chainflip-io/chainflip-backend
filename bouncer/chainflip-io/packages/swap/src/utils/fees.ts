import assert from 'assert';
import { getPoolsNetworkFeeHundredthPips } from '@/shared/consts';
import { Asset, assetChains, Assets, chainNativeAssets } from '@/shared/enums';
import { getSwapRate } from '@/shared/rpc';
import { SwapFee } from '@/shared/schemas';
import { getPools } from '@/swap/utils/pools';
import env from '../config/env';

export const ONE_IN_HUNDREDTH_PIPS = 1000000;

export const getPips = (value: string, hundrethPips: number) =>
  (BigInt(value) * BigInt(hundrethPips)) / BigInt(ONE_IN_HUNDREDTH_PIPS);

export const calculateIncludedSwapFees = async (
  srcAsset: Asset,
  destAsset: Asset,
  depositAmount: string,
  intermediateAmount: string | undefined,
  egressAmount: string,
): Promise<SwapFee[]> => {
  const networkFeeHundredthPips = getPoolsNetworkFeeHundredthPips(
    env.CHAINFLIP_NETWORK,
  );
  const pools = await getPools(srcAsset, destAsset);

  if (srcAsset === Assets.USDC) {
    return [
      {
        type: 'NETWORK',
        chain: assetChains[Assets.USDC],
        asset: Assets.USDC,
        amount: getPips(depositAmount, networkFeeHundredthPips).toString(),
      },
      {
        type: 'LIQUIDITY',
        chain: assetChains[srcAsset],
        asset: srcAsset,
        amount: getPips(
          depositAmount,
          pools[0].liquidityFeeHundredthPips,
        ).toString(),
      },
    ];
  }

  if (destAsset === Assets.USDC) {
    const stableAmountBeforeNetworkFee =
      (BigInt(egressAmount) * BigInt(ONE_IN_HUNDREDTH_PIPS)) /
      BigInt(ONE_IN_HUNDREDTH_PIPS - networkFeeHundredthPips);

    return [
      {
        type: 'NETWORK',
        chain: assetChains[Assets.USDC],
        asset: Assets.USDC,
        amount: getPips(
          String(stableAmountBeforeNetworkFee),
          networkFeeHundredthPips,
        ).toString(),
      },
      {
        type: 'LIQUIDITY',
        chain: assetChains[srcAsset],
        asset: srcAsset,
        amount: getPips(
          depositAmount,
          pools[0].liquidityFeeHundredthPips,
        ).toString(),
      },
    ];
  }

  assert(intermediateAmount, 'no intermediate amount given');

  return [
    {
      type: 'NETWORK',
      chain: assetChains[Assets.USDC],
      asset: Assets.USDC,
      amount: getPips(intermediateAmount, networkFeeHundredthPips).toString(),
    },
    {
      type: 'LIQUIDITY',
      chain: assetChains[srcAsset],
      asset: srcAsset,
      amount: getPips(
        depositAmount,
        pools[0].liquidityFeeHundredthPips,
      ).toString(),
    },
    {
      type: 'LIQUIDITY',
      chain: assetChains[Assets.USDC],
      asset: Assets.USDC,
      amount: getPips(
        intermediateAmount,
        pools[1].liquidityFeeHundredthPips,
      ).toString(),
    },
  ];
};

export const estimateIngressEgressFeeAssetAmount = async (
  nativeFeeAmount: bigint,
  asset: Asset,
  blockHash: string | undefined = undefined,
): Promise<bigint> => {
  const nativeAsset = chainNativeAssets[assetChains[asset]];
  if (asset === nativeAsset) return nativeFeeAmount;

  // TODO: we get the output amount for the "nativeAmount" instead of figuring out the required input amount
  // this makes the result different to the backend if there are limit orders that affect the price in one direction
  // https://github.com/chainflip-io/chainflip-backend/blob/4318931178a1696866e1e70e65d73d722bee4afd/state-chain/pallets/cf-pools/src/lib.rs#L2025
  const rate = await getSwapRate(
    { rpcUrl: env.RPC_NODE_HTTP_URL },
    nativeAsset,
    asset,
    `0x${nativeFeeAmount.toString(16)}`,
    blockHash,
  );

  return rate.output;
};
