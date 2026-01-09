import { z } from 'zod';
import { accountId, hexString } from '../common';

export const ethereumIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});
