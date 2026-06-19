import { z } from 'zod';
import { palletCfPoolsCloseOrder } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsOrderDeletionFailed = z.object({ order: palletCfPoolsCloseOrder });

export const liquidityPoolsOrderDeletionFailedEvent = defineEvent(
  'LiquidityPools.OrderDeletionFailed',
  liquidityPoolsOrderDeletionFailed,
);
