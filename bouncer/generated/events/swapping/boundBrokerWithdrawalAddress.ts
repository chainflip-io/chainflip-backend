import { z } from 'zod';
import { accountId, hexString } from '../common';

export const swappingBoundBrokerWithdrawalAddress = z.object({
  broker: accountId,
  address: hexString,
});
