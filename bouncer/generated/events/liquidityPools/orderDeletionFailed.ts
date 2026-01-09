import { z } from 'zod';
import { palletCfPoolsCloseOrder } from '../common';

export const liquidityPoolsOrderDeletionFailed = z.object({ order: palletCfPoolsCloseOrder });
