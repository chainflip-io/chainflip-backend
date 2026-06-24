import { z } from 'zod';
import { cfPrimitivesChainsAssetsHubAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsHubAsset,
});

export const assethubIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'AssethubIngressEgress.DepositFetchesScheduled',
  assethubIngressEgressDepositFetchesScheduled,
);
