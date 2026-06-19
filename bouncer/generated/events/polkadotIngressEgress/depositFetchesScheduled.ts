import { z } from 'zod';
import { cfPrimitivesChainsAssetsDotAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsDotAsset,
});

export const polkadotIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'PolkadotIngressEgress.DepositFetchesScheduled',
  polkadotIngressEgressDepositFetchesScheduled,
);
