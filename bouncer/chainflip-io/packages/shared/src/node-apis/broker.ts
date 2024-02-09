import { u8aToHex } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import type { Logger } from 'winston';
import { z } from 'zod';
import { Asset, Assets, Chain, Chains } from '../enums';
import { isNotNullish } from '../guards';
import {
  hexString,
  numericString,
  btcAddress,
  dotAddress,
  chainflipAsset,
  chainflipChain,
} from '../parsers';
import { CcmMetadata, ccmMetadataSchema } from '../schemas';
import {
  CamelCaseToSnakeCase,
  camelToSnakeCase,
  transformAsset,
} from '../strings';
import RpcClient from './RpcClient';

type NewSwapRequest = {
  srcAsset: Asset;
  destAsset: Asset;
  srcChain: Chain;
  destChain: Chain;
  destAddress: string;
  ccmMetadata?: CcmMetadata;
};

type SnakeCaseKeys<T> = {
  [K in keyof T as K extends string ? CamelCaseToSnakeCase<K> : K]: T[K];
};

const transformObjToSnakeCase = <T>(
  obj: T | undefined,
): SnakeCaseKeys<T> | undefined => {
  if (!obj) return undefined;
  const newObj: Record<string, unknown> = {};
  for (const key in obj) {
    if (Object.prototype.hasOwnProperty.call(obj, key)) {
      newObj[camelToSnakeCase(key)] = obj[key];
    }
  }
  return newObj as SnakeCaseKeys<T>;
};

const submitAddress = (asset: Asset, address: string): string => {
  if (asset === Assets.DOT) {
    return u8aToHex(decodeAddress(address));
  }
  return address;
};

// hardcoded for now -- remove when source_address is removed from ccmMetadata
const sourceAddress = (chain: Chain): string => {
  if (chain === Chains.Polkadot) {
    return u8aToHex(
      decodeAddress('5GrpknVvGGrGH3EFuURXeMrWHvbpj3VfER1oX5jFtuGbfzCE'),
    );
  }
  if (chain === Chains.Ethereum) {
    return '0x8ba1f109551bd432803012645ac136ddd64dba72';
  }
  if (chain === Chains.Bitcoin) {
    // TODO: fix when broker api is fixed
  }
  return '0x';
};

const requestValidators = {
  requestSwapDepositAddress: z
    .tuple([
      chainflipAsset.transform(transformAsset),
      chainflipAsset.transform(transformAsset),
      z.union([numericString, hexString, btcAddress]),
      z.number(),
      ccmMetadataSchema
        .merge(
          z.object({
            source_chain: chainflipChain,
            source_address: z.union([hexString, btcAddress, dotAddress]),
          }),
        )
        .optional(),
    ])
    .transform(([a, b, c, d, e]) =>
      [a, b, c, d, transformObjToSnakeCase(e)].filter(isNotNullish),
    ),
};

const responseValidators = {
  requestSwapDepositAddress: z
    .object({
      address: z.union([hexString, btcAddress, dotAddress]),
      expiry_block: z.number(),
      issued_block: z.number(),
      channel_id: z.number(),
    })
    .transform(({ address, expiry_block, issued_block, channel_id }) => ({
      address,
      expiryBlock: expiry_block,
      issuedBlock: issued_block,
      channelId: BigInt(channel_id),
    })),
};

export type DepositChannelResponse = z.infer<
  (typeof responseValidators)['requestSwapDepositAddress']
>;

type BrokerClientOpts = {
  url?: string;
  logger?: Logger;
};

export default class BrokerClient extends RpcClient<
  typeof requestValidators,
  typeof responseValidators
> {
  static create(opts: BrokerClientOpts = {}): Promise<BrokerClient> {
    return new BrokerClient(opts).connect();
  }

  private constructor(opts: BrokerClientOpts = {}) {
    super(
      opts.url ?? (process.env.RPC_BROKER_WSS_URL as string),
      requestValidators,
      responseValidators,
      'broker',
      opts.logger,
    );
  }

  async requestSwapDepositAddress(
    swapRequest: NewSwapRequest,
  ): Promise<DepositChannelResponse> {
    const { srcAsset, destAsset, destAddress, srcChain } = swapRequest;

    const depositChannelResponse = await this.sendRequest(
      'requestSwapDepositAddress',
      srcAsset,
      destAsset,
      submitAddress(srcAsset, destAddress),
      0,
      swapRequest.ccmMetadata && {
        ...swapRequest.ccmMetadata,
        source_chain: srcChain,
        source_address: sourceAddress(srcChain),
      },
    );

    return depositChannelResponse;
  }
}
