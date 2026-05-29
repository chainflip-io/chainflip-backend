import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentUpdatedArbAsset = z.tuple([cfPrimitivesChainsAssetsArbAsset, hexString]);

export const environmentUpdatedArbAssetEvent = defineEvent(
  'Environment.UpdatedArbAsset',
  environmentUpdatedArbAsset,
);
