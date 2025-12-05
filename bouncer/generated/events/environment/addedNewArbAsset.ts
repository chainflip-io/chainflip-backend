import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset, hexString } from '../common';

export const environmentAddedNewArbAsset = z.tuple([cfPrimitivesChainsAssetsArbAsset, hexString]);
