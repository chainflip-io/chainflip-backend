import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});

export const assethubBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'AssethubBroadcaster.TransactionFeeDeficitRecorded',
  assethubBroadcasterTransactionFeeDeficitRecorded,
);
