import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingWithdrawalRequested = z.object({
  accountId,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  egressAsset: cfPrimitivesChainsAssetsAnyAsset,
  egressAmount: numberOrHex,
  egressFee: numberOrHex,
  destinationAddress: cfChainsAddressEncodedAddress,
});

export const swappingWithdrawalRequestedEvent = defineEvent(
  'Swapping.WithdrawalRequested',
  swappingWithdrawalRequested,
);
