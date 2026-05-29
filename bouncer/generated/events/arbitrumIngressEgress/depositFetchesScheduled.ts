import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsArbAsset,
});

export const arbitrumIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'ArbitrumIngressEgress.DepositFetchesScheduled',
  arbitrumIngressEgressDepositFetchesScheduled,
);
