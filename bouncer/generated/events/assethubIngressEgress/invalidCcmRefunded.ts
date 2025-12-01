import { z } from 'zod';
import { cfPrimitivesChainsAssetsHubAsset, hexString, numberOrHex } from '../common';

export const assethubIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsHubAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});
