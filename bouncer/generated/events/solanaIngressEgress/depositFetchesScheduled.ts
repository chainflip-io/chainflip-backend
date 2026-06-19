import { z } from 'zod';
import { cfPrimitivesChainsAssetsSolAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsSolAsset,
});

export const solanaIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'SolanaIngressEgress.DepositFetchesScheduled',
  solanaIngressEgressDepositFetchesScheduled,
);
