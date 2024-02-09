import { UncheckedAssetAndChain } from '@/shared/enums';
import { getEnvironment } from '@/shared/rpc';
import {
  readAssetValue,
  validateSwapAmount as validateAmount,
} from '@/shared/rpc/utils';
import { memoize } from './function';
import env from '../config/env';

const cachedGetEnvironment = memoize(getEnvironment, 6_000);

type Result = { success: true } | { success: false; reason: string };

const rpcConfig = { rpcUrl: env.RPC_NODE_HTTP_URL };

export const validateSwapAmount = async (
  asset: UncheckedAssetAndChain,
  amount: bigint,
): Promise<Result> => {
  const environment = await cachedGetEnvironment(rpcConfig);

  return validateAmount(environment, asset, amount);
};

export const getMinimumEgressAmount = async (
  asset: UncheckedAssetAndChain,
): Promise<bigint> => {
  const environment = await cachedGetEnvironment(rpcConfig);

  return readAssetValue(environment.ingressEgress.minimumEgressAmounts, asset);
};

export const getNativeIngressFee = async (
  asset: UncheckedAssetAndChain,
): Promise<bigint> => {
  const environment = await cachedGetEnvironment(rpcConfig);

  return readAssetValue(environment.ingressEgress.ingressFees, asset);
};

export const getNativeEgressFee = async (
  asset: UncheckedAssetAndChain,
): Promise<bigint> => {
  const environment = await cachedGetEnvironment(rpcConfig);

  return readAssetValue(environment.ingressEgress.egressFees, asset);
};
