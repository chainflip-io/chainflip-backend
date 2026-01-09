import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset, hexString } from '../common';

export const environmentAddedNewEthAsset = z.tuple([cfPrimitivesChainsAssetsEthAsset, hexString]);
