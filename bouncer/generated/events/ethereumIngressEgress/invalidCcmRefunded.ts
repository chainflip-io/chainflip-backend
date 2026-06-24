import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsEthAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});

export const ethereumIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'EthereumIngressEgress.InvalidCcmRefunded',
  ethereumIngressEgressInvalidCcmRefunded,
);
