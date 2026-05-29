import { z } from 'zod';
import { cfPrimitivesChainsAssetsDotAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsDotAsset,
  disabled: z.boolean(),
});

export const polkadotIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'PolkadotIngressEgress.AssetEgressStatusChanged',
  polkadotIngressEgressAssetEgressStatusChanged,
);
