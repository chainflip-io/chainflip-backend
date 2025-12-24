import { z } from 'zod';
import { accountId, hexString } from '../common';

export const assethubIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});
