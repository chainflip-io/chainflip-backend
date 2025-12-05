import { z } from 'zod';
import { accountId, hexString } from '../common';

export const solanaIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});
