import { z } from 'zod';
import { cfPrimitivesChainsAssetsTronAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsTronAsset,
  disabled: z.boolean(),
});

export const tronIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'TronIngressEgress.AssetEgressStatusChanged',
  tronIngressEgressAssetEgressStatusChanged,
);
