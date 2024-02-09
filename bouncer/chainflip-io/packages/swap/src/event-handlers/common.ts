import { z } from 'zod';
import { assetChains } from '@/shared/enums';
import {
  btcAddress,
  chainflipChain,
  dotAddress,
  hexString,
  unsignedInteger,
} from '@/shared/parsers';

export const egressId = z.tuple([
  z.object({ __kind: chainflipChain }).transform(({ __kind }) => __kind),
  unsignedInteger,
]);

const ethChainAddress = z.object({
  __kind: z.literal('Eth'),
  value: hexString,
});
const dotChainAddress = z.object({
  __kind: z.literal('Dot'),
  value: dotAddress,
});
const btcChainAddress = z.object({
  __kind: z.literal('Btc'),
  value: hexString
    .transform((v) => Buffer.from(v.slice(2), 'hex').toString())
    .pipe(btcAddress),
});

export const encodedAddress = z
  .union([ethChainAddress, dotChainAddress, btcChainAddress])
  .transform(
    ({ __kind, value }) =>
      ({
        chain: assetChains[__kind.toUpperCase() as Uppercase<typeof __kind>],
        address: value,
      } as const),
  );
