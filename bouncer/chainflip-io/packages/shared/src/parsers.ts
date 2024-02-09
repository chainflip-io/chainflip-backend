import { hexToU8a } from '@polkadot/util';
import { decodeAddress, encodeAddress } from '@polkadot/util-crypto';
import * as ethers from 'ethers';
import { z, ZodErrorMap } from 'zod';
import type { Asset } from './enums';
import { Assets, ChainflipNetworks, Chains } from './enums';
import { isString } from './guards';
import {
  validateBitcoinMainnetAddress,
  validateBitcoinRegtestAddress,
  validateBitcoinTestnetAddress,
} from './validation/addressValidation';

const errorMap: ZodErrorMap = (issue, context) => ({
  message: `received: ${JSON.stringify(context.data)}`,
});

export const string = z.string({ errorMap });
export const number = z.number({ errorMap });
export const numericString = string.regex(/^[0-9]+$/);
export const hexString = z.custom<`0x${string}`>(
  (val) => typeof val === 'string' && /^0x[0-9a-f]+$/i.test(val),
);
export const hexStringFromNumber = numericString
  .transform((arg) => ethers.BigNumber.from(arg).toHexString())
  .refine((arg) => arg.startsWith('0x'));
export const bareHexString = string.regex(/^[0-9a-f]+$/);
export const btcAddress = z.union([
  string.regex(/^(1|3|bc1)/).refine(validateBitcoinMainnetAddress),
  string.regex(/^(m|n|2|tb1)/).refine(validateBitcoinTestnetAddress),
  string.regex(/^bcrt1/).refine(validateBitcoinRegtestAddress),
]);

export const dotAddress = z
  .union([string, hexString])
  .transform((arg) => {
    try {
      if (arg.startsWith('0x')) {
        return encodeAddress(hexToU8a(arg));
      }
      // this will throw if the address is invalid
      decodeAddress(arg);
      return arg;
    } catch {
      return null;
    }
  })
  .refine(isString);

export const ethereumAddress = hexString.refine((address) =>
  ethers.utils.isAddress(address),
);

export const u64 = numericString.transform((arg) => BigInt(arg));

export const u128 = z
  .union([numericString, hexString])
  .transform((arg) => BigInt(arg));

export const unsignedInteger = z.union([
  u128,
  z.number().transform((n) => BigInt(n)),
]);

export const chainflipAssetEnum = z
  .object({ __kind: z.enum(['Usdc', 'Flip', 'Dot', 'Eth', 'Btc']) })
  .transform(({ __kind }) => __kind.toUpperCase() as Asset);

export const chainflipChain = z.nativeEnum(Chains);
export const chainflipAsset = z.nativeEnum(Assets);
export const chainflipNetwork = z.nativeEnum(ChainflipNetworks);
