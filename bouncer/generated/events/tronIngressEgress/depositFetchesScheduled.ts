import { z } from 'zod';
import { cfPrimitivesChainsAssetsTronAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsTronAsset,
});

export const tronIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'TronIngressEgress.DepositFetchesScheduled',
  tronIngressEgressDepositFetchesScheduled,
);
