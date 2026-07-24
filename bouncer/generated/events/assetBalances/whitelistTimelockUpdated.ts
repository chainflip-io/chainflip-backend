import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesWhitelistTimelockUpdated = z.object({
  accountId,
  duration: numberOrHex,
  effectiveAt: numberOrHex,
});

export const assetBalancesWhitelistTimelockUpdatedEvent = defineEvent(
  'AssetBalances.WhitelistTimelockUpdated',
  assetBalancesWhitelistTimelockUpdated,
);
