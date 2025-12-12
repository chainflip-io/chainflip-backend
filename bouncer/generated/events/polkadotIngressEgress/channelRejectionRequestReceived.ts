import { z } from 'zod';
import { accountId, hexString } from '../common';

export const polkadotIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});
