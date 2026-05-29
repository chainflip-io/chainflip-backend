import { z } from 'zod';
import { cfChainsBtcScriptPubkey, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: cfChainsBtcScriptPubkey,
  amount: numberOrHex,
});

export const bitcoinBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'BitcoinBroadcaster.TransactionFeeDeficitRecorded',
  bitcoinBroadcasterTransactionFeeDeficitRecorded,
);
