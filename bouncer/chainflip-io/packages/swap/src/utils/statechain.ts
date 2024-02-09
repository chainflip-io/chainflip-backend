import { z } from 'zod';
import { AssetAndChain } from '@/shared/enums';
import RpcClient from '@/shared/node-apis/RpcClient';
import { chainflipAssetAndChain, hexStringFromNumber } from '@/shared/parsers';
import { ParsedQuoteParams } from '@/shared/schemas';
import { memoize } from './function';
import env from '../config/env';
import { swapRateResponseSchema } from '../quoting/schemas';

const requestValidators = {
  swap_rate: z.tuple([
    chainflipAssetAndChain,
    chainflipAssetAndChain,
    hexStringFromNumber,
  ]),
};

const responseValidators = {
  swap_rate: swapRateResponseSchema,
};

const initializeClient = memoize(async () => {
  const rpcClient = await new RpcClient(
    env.RPC_NODE_WSS_URL,
    requestValidators,
    responseValidators,
    'cf',
  ).connect();

  return rpcClient;
});

const getSwapAmount = async (
  srcAsset: AssetAndChain,
  destAsset: AssetAndChain,
  amount: string,
): Promise<z.output<(typeof responseValidators)['swap_rate']>> => {
  const client = await initializeClient();

  return client.sendRequest('swap_rate', srcAsset, destAsset, amount);
};

export const getBrokerQuote = async (
  { srcAsset, destAsset, amount }: ParsedQuoteParams,
  id: string,
) => {
  const quote = await getSwapAmount(srcAsset, destAsset, amount);

  return { id, ...quote };
};
