import { decodeAddress } from '@polkadot/util-crypto';
import { z } from 'zod';
import { Assets, ChainflipNetwork, Chains } from '../enums';
import {
  btcAddress,
  dotAddress,
  ethereumAddress,
  numericString,
} from '../parsers';
import { ccmMetadataSchema } from '../schemas';

const bytesToHex = (arr: Uint8Array | number[]) =>
  `0x${[...arr].map((v) => v.toString(16).padStart(2, '0')).join('')}`;

const utf8ToHex = (str: string) => `0x${Buffer.from(str).toString('hex')}`;

const eth = z.object({
  amount: numericString,
  srcChain: z.literal(Chains.Ethereum),
  srcAsset: z.literal(Assets.ETH),
});

const ethToEthereum = eth.extend({
  destChain: z.literal(Chains.Ethereum),
  destAddress: ethereumAddress,
});

const ethToDot = eth.extend({
  destChain: z.literal(Chains.Polkadot),
  destAddress: dotAddress.transform((addr) => bytesToHex(decodeAddress(addr))),
  destAsset: z.literal(Assets.DOT),
});

const ethToBtc = (network: ChainflipNetwork) =>
  eth.extend({
    destChain: z.literal(Chains.Bitcoin),
    destAddress: btcAddress(network).transform(utf8ToHex),
    destAsset: z.literal(Assets.BTC),
  });

const erc20Asset = z.union([z.literal(Assets.FLIP), z.literal(Assets.USDC)]);

const ethToERC20 = ethToEthereum.extend({ destAsset: erc20Asset });

const nativeSwapParamsSchema = (network: ChainflipNetwork) =>
  z.union([ethToERC20, ethToDot, ethToBtc(network)]);

export type NativeSwapParams = z.infer<
  ReturnType<typeof nativeSwapParamsSchema>
>;

const flipToEthereumAsset = ethToEthereum.extend({
  srcAsset: z.literal(Assets.FLIP),
  destAsset: z.union([z.literal(Assets.USDC), z.literal(Assets.ETH)]),
});

const usdcToEthereumAsset = ethToEthereum.extend({
  srcAsset: z.literal(Assets.USDC),
  destAsset: z.union([z.literal(Assets.FLIP), z.literal(Assets.ETH)]),
});

const erc20ToDot = ethToDot.extend({ srcAsset: erc20Asset });

const erc20ToBtc = (network: ChainflipNetwork) =>
  ethToBtc(network).extend({ srcAsset: erc20Asset });

const tokenSwapParamsSchema = (network: ChainflipNetwork) =>
  z.union([
    flipToEthereumAsset,
    usdcToEthereumAsset,
    erc20ToDot,
    erc20ToBtc(network),
  ]);

const ccmFlipToEthereumAssset = flipToEthereumAsset.extend({
  ccmMetadata: ccmMetadataSchema,
});

const ccmUsdcToEthereumAsset = usdcToEthereumAsset.extend({
  ccmMetadata: ccmMetadataSchema,
});

const tokenCallParamsSchema = z.union([
  ccmFlipToEthereumAssset,
  ccmUsdcToEthereumAsset,
]);

const nativeCallParamsSchema = ethToERC20.extend({
  ccmMetadata: ccmMetadataSchema,
});

export const executeSwapParamsSchema = (network: ChainflipNetwork) =>
  z.union([
    // call schemas needs to precede swap schemas
    nativeCallParamsSchema,
    tokenCallParamsSchema,
    nativeSwapParamsSchema(network),
    tokenSwapParamsSchema(network),
  ]);

export type ExecuteSwapParams = z.infer<
  ReturnType<typeof executeSwapParamsSchema>
>;
export type NativeCallParams = z.infer<typeof nativeCallParamsSchema>;
export type TokenCallParams = z.infer<typeof tokenCallParamsSchema>;
export type TokenSwapParams = z.infer<ReturnType<typeof tokenSwapParamsSchema>>;
