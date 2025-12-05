import { z } from 'zod';
import { accountId, palletCfValidatorDelegationChange } from '../common';

export const validatorMaxBidUpdated = z.object({
  delegator: accountId,
  change: palletCfValidatorDelegationChange,
});
