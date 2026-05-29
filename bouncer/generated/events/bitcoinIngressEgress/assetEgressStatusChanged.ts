import { z } from 'zod';
import { cfPrimitivesChainsAssetsBtcAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsBtcAsset,
  disabled: z.boolean(),
});

export const bitcoinIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'BitcoinIngressEgress.AssetEgressStatusChanged',
  bitcoinIngressEgressAssetEgressStatusChanged,
);
