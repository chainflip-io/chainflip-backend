import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';

export const swappingWithdrawalRequested = z.object({
  accountId,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  egressAsset: cfPrimitivesChainsAssetsAnyAsset,
  egressAmount: numberOrHex,
  egressFee: numberOrHex,
  destinationAddress: cfChainsAddressEncodedAddress,
});
