import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsArbAsset,
  disabled: z.boolean(),
});

export const arbitrumIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'ArbitrumIngressEgress.AssetEgressStatusChanged',
  arbitrumIngressEgressAssetEgressStatusChanged,
);
