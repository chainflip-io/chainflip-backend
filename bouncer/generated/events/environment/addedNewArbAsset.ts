import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentAddedNewArbAsset = z.tuple([cfPrimitivesChainsAssetsArbAsset, hexString]);

export const environmentAddedNewArbAssetEvent = defineEvent(
  'Environment.AddedNewArbAsset',
  environmentAddedNewArbAsset,
);
