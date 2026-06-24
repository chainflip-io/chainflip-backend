import { z } from 'zod';
import { cfPrimitivesChainsAssetsBscAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsBscAsset,
  disabled: z.boolean(),
});

export const bscIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'BscIngressEgress.AssetEgressStatusChanged',
  bscIngressEgressAssetEgressStatusChanged,
);
