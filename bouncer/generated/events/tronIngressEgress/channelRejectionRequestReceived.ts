import { z } from 'zod';
import { accountId, hexString } from '../common';

export const tronIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});
