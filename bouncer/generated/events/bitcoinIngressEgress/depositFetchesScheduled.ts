import { z } from 'zod';
import { cfPrimitivesChainsAssetsBtcAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsBtcAsset,
});

export const bitcoinIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'BitcoinIngressEgress.DepositFetchesScheduled',
  bitcoinIngressEgressDepositFetchesScheduled,
);
