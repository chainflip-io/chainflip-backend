import { z } from 'zod';
import { cfPrimitivesChainsAssetsHubAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsHubAsset,
  disabled: z.boolean(),
});

export const assethubIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'AssethubIngressEgress.AssetEgressStatusChanged',
  assethubIngressEgressAssetEgressStatusChanged,
);
