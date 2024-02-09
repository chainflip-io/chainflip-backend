import axios from 'axios';
import { z } from 'zod';
import { ChainflipNetwork, ChainflipNetworks } from '../enums';

const numberOrHex = z
  .union([z.string(), z.number()])
  .transform((str) => BigInt(str));

type CamelCase<T> = T extends string
  ? T extends `${infer F}_${infer R}`
    ? `${F}${Capitalize<CamelCase<R>>}`
    : T
  : never;

type CamelCaseRecord<T> =
  T extends Record<string, unknown>
    ? { [K in keyof T as CamelCase<K>]: CamelCaseRecord<T[K]> }
    : T;

const camelCase = <T extends string>(str: T): CamelCase<T> =>
  str.replace(/_([a-z])/g, (_, char) => char.toUpperCase()) as CamelCase<T>;

const camelCaseKeys = <T>(obj: T): CamelCaseRecord<T> => {
  if (typeof obj !== 'object' || obj === null) return obj as CamelCaseRecord<T>;

  return Object.fromEntries(
    Object.entries(obj).map(([key, value]) => [
      camelCase(key),
      camelCaseKeys(value),
    ]),
  ) as CamelCaseRecord<T>;
};

const RPC_URLS: Record<ChainflipNetwork, string> = {
  [ChainflipNetworks.backspin]: 'https://backspin-rpc.staging',
  [ChainflipNetworks.sisyphos]: 'https://sisyphos.chainflip.xyz',
  [ChainflipNetworks.perseverance]: 'https://perseverance.chainflip.xyz',
  [ChainflipNetworks.mainnet]: 'https://mainnet-rpc.chainflip.io',
};

export type RpcConfig = { rpcUrl: string } | { network: ChainflipNetwork };

type RpcParams = {
  cf_environment: [at?: string];
  cf_swapping_environment: [at?: string];
  cf_ingress_egress_environment: [at?: string];
  cf_funding_environment: [at?: string];
  cf_pool_info: [at?: string];
  cf_swap_rate: [
    fromAsset: string,
    toAsset: string,
    amount: `0x${string}`,
    at?: string,
  ];
};

type RpcMethod = keyof RpcParams;

const createRequest =
  <M extends RpcMethod, R extends z.ZodTypeAny>(method: M, responseParser: R) =>
  async (
    urlOrNetwork: RpcConfig,
    ...params: RpcParams[M]
  ): Promise<CamelCaseRecord<z.output<R>>> => {
    const url =
      'network' in urlOrNetwork
        ? RPC_URLS[urlOrNetwork.network]
        : urlOrNetwork.rpcUrl;
    const { data } = await axios.post(url, {
      jsonrpc: '2.0',
      method,
      params,
      id: 1,
    });

    const result = responseParser.safeParse(data.result);

    if (result.success) {
      return camelCaseKeys(result.data);
    }

    throw new Error(`RPC request "${method}" failed`, { cause: data.error });
  };

const fundingEnvironment = z.object({
  redemption_tax: numberOrHex,
  minimum_funding_amount: numberOrHex,
});
export const getFundingEnvironment = createRequest(
  'cf_funding_environment',
  fundingEnvironment,
);

const chainAssetMap = <Z extends z.ZodTypeAny>(parser: Z) =>
  z.object({
    Bitcoin: z.object({ BTC: parser }),
    Ethereum: z.object({ ETH: parser, USDC: parser, FLIP: parser }),
    Polkadot: z.object({ DOT: parser }),
  });

export type ChainAssetMap<T> = {
  Bitcoin: {
    BTC: T;
  };
  Ethereum: {
    ETH: T;
    USDC: T;
    FLIP: T;
  };
  Polkadot: {
    DOT: T;
  };
};

const chainAssetNumberMap = chainAssetMap(numberOrHex);
const chainAssetNumberNullableMap = chainAssetMap(numberOrHex.nullable());

const swappingEnvironment = z.object({
  maximum_swap_amounts: chainAssetNumberNullableMap,
});

export const getSwappingEnvironment = createRequest(
  'cf_swapping_environment',
  swappingEnvironment,
);

type Rename<T, U extends Record<string, string>> = Omit<T, keyof U> & {
  [K in keyof U as NonNullable<U[K]>]: K extends keyof T ? T[K] : never;
};

const rename =
  <const U extends Record<string, string>>(mapping: U) =>
  <T>(obj: T): Rename<T, U> =>
    Object.fromEntries(
      Object.entries(obj as Record<string, unknown>).map(([key, value]) => [
        key in mapping ? mapping[key] : key,
        value,
      ]),
    ) as Rename<T, U>;

const ingressEgressEnvironment = z
  .object({
    minimum_deposit_amounts: chainAssetNumberMap,
    ingress_fees: chainAssetNumberMap,
    egress_fees: chainAssetNumberMap,
    // TODO(1.2): remove optional and default value
    egress_dust_limits: chainAssetNumberMap.optional().default({
      Bitcoin: { BTC: 0x258 },
      Ethereum: { ETH: 0x1, USDC: 0x1, FLIP: 0x1 },
      Polkadot: { DOT: 0x1 },
    }),
  })
  .transform(rename({ egress_dust_limits: 'minimum_egress_amounts' }));

export const getIngressEgressEnvironment = createRequest(
  'cf_ingress_egress_environment',
  ingressEgressEnvironment,
);

const rpcAsset = z.union([
  z.literal('BTC'),
  z.object({ chain: z.literal('Bitcoin'), asset: z.literal('BTC') }),
  z.literal('DOT'),
  z.object({ chain: z.literal('Polkadot'), asset: z.literal('DOT') }),
  z.literal('FLIP'),
  z.object({ chain: z.literal('Ethereum'), asset: z.literal('FLIP') }),
  z.literal('ETH'),
  z.object({ chain: z.literal('Ethereum'), asset: z.literal('ETH') }),
  z.literal('USDC'),
  z.object({ chain: z.literal('Ethereum'), asset: z.literal('USDC') }),
]);

const poolInfo = z.intersection(
  z.object({
    limit_order_fee_hundredth_pips: z.number(),
    range_order_fee_hundredth_pips: z.number(),
  }),
  z.union([
    z.object({ quote_asset: rpcAsset }),
    z
      .object({ pair_asset: rpcAsset })
      .transform(({ pair_asset }) => ({ quote_asset: pair_asset })),
  ]),
);

const feesInfo = z.object({
  Bitcoin: z.object({ BTC: poolInfo }),
  Ethereum: z.object({ ETH: poolInfo, FLIP: poolInfo }),
  Polkadot: z.object({ DOT: poolInfo }),
});

const poolsEnvironment = z.object({ fees: feesInfo });

export const getPoolsEnvironment = createRequest(
  'cf_pool_info',
  poolsEnvironment,
);

const environment = z.object({
  ingress_egress: ingressEgressEnvironment,
  swapping: swappingEnvironment,
  funding: fundingEnvironment,
  // pools: poolsEnvironment,
});

export const getEnvironment = createRequest('cf_environment', environment);

export type RpcEnvironment = z.input<typeof environment>;

export type Environment = Awaited<ReturnType<typeof getEnvironment>>;

const swapRate = z.object({
  output: numberOrHex,
});
export const getSwapRate = createRequest('cf_swap_rate', swapRate);
