import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset, hexString, numberOrHex } from '../common';

export const ethereumIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsEthAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});
