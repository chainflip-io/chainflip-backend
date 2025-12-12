import { z } from 'zod';
import {
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';

export const liquidityProviderWithdrawalEgressScheduled = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  destinationAddress: cfChainsAddressEncodedAddress,
  fee: numberOrHex,
});
