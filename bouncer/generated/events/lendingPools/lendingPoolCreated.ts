import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLendingPoolCreated = z.object({ asset: cfPrimitivesChainsAssetsAnyAsset });

export const lendingPoolsLendingPoolCreatedEvent = defineEvent(
  'LendingPools.LendingPoolCreated',
  lendingPoolsLendingPoolCreated,
);
