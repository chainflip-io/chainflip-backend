import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset } from '../common';

export const lendingPoolsLendingPoolCreated = z.object({ asset: cfPrimitivesChainsAssetsAnyAsset });
