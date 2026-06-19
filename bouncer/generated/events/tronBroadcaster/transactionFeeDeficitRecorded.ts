import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});

export const tronBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'TronBroadcaster.TransactionFeeDeficitRecorded',
  tronBroadcasterTransactionFeeDeficitRecorded,
);
