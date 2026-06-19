import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsArbAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});

export const arbitrumIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'ArbitrumIngressEgress.InvalidCcmRefunded',
  arbitrumIngressEgressInvalidCcmRefunded,
);
