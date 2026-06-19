import { z } from 'zod';
import { cfPrimitivesChainsAssetsHubAsset, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsHubAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});

export const assethubIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'AssethubIngressEgress.InvalidCcmRefunded',
  assethubIngressEgressInvalidCcmRefunded,
);
