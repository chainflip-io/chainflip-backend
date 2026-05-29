import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsEthAsset,
});

export const ethereumIngressEgressDepositFetchesScheduledEvent = defineEvent(
  'EthereumIngressEgress.DepositFetchesScheduled',
  ethereumIngressEgressDepositFetchesScheduled,
);
