import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsEthAsset,
  disabled: z.boolean(),
});

export const ethereumIngressEgressAssetEgressStatusChangedEvent = defineEvent(
  'EthereumIngressEgress.AssetEgressStatusChanged',
  ethereumIngressEgressAssetEgressStatusChanged,
);
