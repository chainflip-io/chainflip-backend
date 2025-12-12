import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset, hexString } from '../common';

export const environmentUpdatedEthAsset = z.tuple([cfPrimitivesChainsAssetsEthAsset, hexString]);
