import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset, hexString } from '../common';

export const environmentUpdatedArbAsset = z.tuple([cfPrimitivesChainsAssetsArbAsset, hexString]);
