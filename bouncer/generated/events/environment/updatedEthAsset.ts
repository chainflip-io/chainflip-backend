import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentUpdatedEthAsset = z.tuple([cfPrimitivesChainsAssetsEthAsset, hexString]);

export const environmentUpdatedEthAssetEvent = defineEvent(
  'Environment.UpdatedEthAsset',
  environmentUpdatedEthAsset,
);
