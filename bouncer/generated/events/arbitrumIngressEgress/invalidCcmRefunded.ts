import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset, hexString, numberOrHex } from '../common';

export const arbitrumIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsArbAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});
