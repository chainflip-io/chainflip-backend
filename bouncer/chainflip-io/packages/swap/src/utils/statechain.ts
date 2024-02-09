import { z } from 'zod';
import { Asset } from '@/shared/enums';
import RpcClient from '@/shared/node-apis/RpcClient';
import { chainflipAsset, hexStringFromNumber } from '@/shared/parsers';
import { transformAsset } from '@/shared/strings';
import { QuoteQueryResponse, QuoteQueryParams } from '../schemas';
import { memoize } from './function';

const requestValidators = {
  swap_rate: z.tuple([
    chainflipAsset.transform(transformAsset),
    chainflipAsset.transform(transformAsset),
    hexStringFromNumber,
  ]),
};

// parse hex encoding or decimal encoding into decimal encoding
const assetAmount = z.string().transform((v) => BigInt(v).toString());

const responseValidators = {
  swap_rate: z.object({
    // TODO: simplify when we know how Rust `Option` is encoded
    intermediary: assetAmount.optional().nullable(),
    output: assetAmount,
  }),
};

const initializeClient = memoize(async () => {
  const rpcClient = await new RpcClient(
    process.env.RPC_NODE_WSS_URL as string,
    requestValidators,
    responseValidators,
    'cf',
  ).connect();

  return rpcClient;
});

const getSwapAmount = async (
  srcAsset: Asset,
  destAsset: Asset,
  amount: string,
): Promise<z.infer<(typeof responseValidators)['swap_rate']>> => {
  const client = await initializeClient();

  return client.sendRequest('swap_rate', srcAsset, destAsset, amount);
};

export const getBrokerQuote = async (
  { srcAsset, destAsset, amount }: QuoteQueryParams,
  id: string,
): Promise<QuoteQueryResponse> => {
  const { intermediary, output } = await getSwapAmount(
    srcAsset,
    destAsset,
    amount,
  );

  return {
    id,
    intermediateAmount: intermediary ?? undefined,
    egressAmount: output,
  };
};
