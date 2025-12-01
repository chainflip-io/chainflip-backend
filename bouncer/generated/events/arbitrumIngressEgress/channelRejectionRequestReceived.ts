import { z } from 'zod';
import { accountId, hexString } from '../common';

export const arbitrumIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});
