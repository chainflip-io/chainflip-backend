import { z } from 'zod';
import { accountId, palletCfValidatorDelegationOperatorSettings } from '../common';

export const validatorOperatorSettingsUpdated = z.object({
  operator: accountId,
  settings: palletCfValidatorDelegationOperatorSettings,
});
