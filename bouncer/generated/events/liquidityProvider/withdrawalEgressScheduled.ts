import { z } from 'zod';
import {
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityProviderWithdrawalEgressScheduled = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  destinationAddress: cfChainsAddressEncodedAddress,
  fee: numberOrHex,
});

export const liquidityProviderWithdrawalEgressScheduledEvent = defineEvent(
  'LiquidityProvider.WithdrawalEgressScheduled',
  liquidityProviderWithdrawalEgressScheduled,
);
