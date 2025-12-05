import { z } from 'zod';
import { accountId, cfChainsBtcScriptPubkey } from '../common';

export const bitcoinIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: cfChainsBtcScriptPubkey,
});
