import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentAddedNewEthAsset = z.tuple([cfPrimitivesChainsAssetsEthAsset, hexString]);

export const environmentAddedNewEthAssetEvent = defineEvent(
  'Environment.AddedNewEthAsset',
  environmentAddedNewEthAsset,
);
