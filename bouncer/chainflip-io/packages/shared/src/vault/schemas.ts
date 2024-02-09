import { decodeAddress } from '@polkadot/util-crypto';
import { Signer } from 'ethers';
import { z } from 'zod';
import { Assets, Chains } from '../enums';
import {
  btcAddress,
  chainflipNetwork,
  dotAddress,
  ethereumAddress,
  hexString,
  numericString,
} from '../parsers';

const bytesToHex = (arr: Uint8Array | number[]) =>
  `0x${[...arr].map((v) => v.toString(16).padStart(2, '0')).join('')}`;

const utf8ToHex = (str: string) => `0x${Buffer.from(str).toString('hex')}`;

const base = z.object({
  amount: z.union([numericString, hexString, z.bigint()]),
  srcChain: z.literal(Chains.Ethereum),
  srcAsset: z.literal(Assets.ETH),
});

const ethereumBase = base.extend({
  destChain: z.literal(Chains.Ethereum),
  destAddress: ethereumAddress,
});

const polkadotBase = base.extend({
  destChain: z.literal(Chains.Polkadot),
  destAddress: dotAddress.transform((addr) => bytesToHex(decodeAddress(addr))),
});

const bitcoinBase = base.extend({
  destChain: z.literal(Chains.Bitcoin),
  destAddress: btcAddress.transform(utf8ToHex),
});

const erc20 = z.union([z.literal(Assets.FLIP), z.literal(Assets.USDC)]);

const ethereumNative = ethereumBase.extend({ destAsset: erc20 });

const polkadotNative = polkadotBase.extend({
  destAsset: z.literal(Assets.DOT),
});

const bitcoinNative = bitcoinBase.extend({ destAsset: z.literal(Assets.BTC) });

const nativeSwapParamsSchema = z.union([
  ethereumNative,
  polkadotNative,
  bitcoinNative,
]);

export type NativeSwapParams = z.infer<typeof nativeSwapParamsSchema>;

const tokenSwapParamsSchema = z.union([
  ethereumBase.extend({
    srcAsset: z.literal(Assets.FLIP),
    destAsset: z.union([z.literal(Assets.USDC), z.literal(Assets.ETH)]),
  }),
  ethereumBase.extend({
    srcAsset: z.literal(Assets.USDC),
    destAsset: z.union([z.literal(Assets.FLIP), z.literal(Assets.ETH)]),
  }),
  polkadotNative.extend({ srcAsset: erc20 }),
  bitcoinNative.extend({ srcAsset: erc20 }),
]);

export type TokenSwapParams = z.infer<typeof tokenSwapParamsSchema>;

export const executeSwapParamsSchema = z.union([
  nativeSwapParamsSchema,
  tokenSwapParamsSchema,
]);

export type ExecuteSwapParams = z.infer<typeof executeSwapParamsSchema>;

const nativeCallParamsSchema = ethereumBase.extend({
  destAsset: erc20,
  message: z.string(),
  gasAmount: numericString,
});

export type NativeCallParams = z.infer<typeof nativeCallParamsSchema>;

const tokenCallParamsSchema = z.union([
  ethereumBase.extend({
    srcAsset: z.literal(Assets.FLIP),
    destAsset: z.union([z.literal(Assets.USDC), z.literal(Assets.ETH)]),
    message: hexString,
    gasAmount: numericString,
  }),
  ethereumBase.extend({
    srcAsset: z.literal(Assets.USDC),
    destAsset: z.union([z.literal(Assets.FLIP), z.literal(Assets.ETH)]),
    message: hexString,
    gasAmount: numericString,
  }),
]);

export type TokenCallParams = z.infer<typeof tokenCallParamsSchema>;

export const executeCallParamsSchema = z.union([
  nativeCallParamsSchema,
  tokenCallParamsSchema,
]);

export type ExecuteCallParams = z.infer<typeof executeCallParamsSchema>;

export const executeOptionsSchema = z.intersection(
  z.object({
    signer: z.instanceof(Signer),
    nonce: z.union([z.number(), z.bigint(), z.string()]).optional(),
  }),
  z.union([
    z.object({ network: chainflipNetwork }),
    z.object({
      network: z.literal('localnet'),
      vaultContractAddress: z.string(),
      srcTokenContractAddress: z.string().optional(),
    }),
  ]),
);

export type ExecuteOptions = z.infer<typeof executeOptionsSchema>;
