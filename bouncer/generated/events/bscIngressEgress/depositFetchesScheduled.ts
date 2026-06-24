import { z } from 'zod';
import { cfPrimitivesChainsAssetsBscAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsBscAsset,
});

export const bscIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'BscIngressEgress.DepositFetchesScheduled',
  bscIngressEgressDepositFetchesScheduled,
);
