import { z } from 'zod';
import { cfPrimitivesChainsAssetsSolAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsSolAsset,
  disabled: z.boolean(),
});

export const solanaIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'SolanaIngressEgress.AssetEgressStatusChanged',
  solanaIngressEgressAssetEgressStatusChanged,
);
