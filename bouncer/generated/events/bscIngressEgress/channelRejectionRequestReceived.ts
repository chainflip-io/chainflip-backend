import { z } from 'zod';
import { accountId, hexString } from '../common';

export const bscIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});
