import { z } from 'zod';
import { cfChainsBtcScriptPubkey, numberOrHex } from '../common';

export const bitcoinBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: cfChainsBtcScriptPubkey,
  amount: numberOrHex,
});
