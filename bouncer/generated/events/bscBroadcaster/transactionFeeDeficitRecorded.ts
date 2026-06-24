import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});

export const bscBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'BscBroadcaster.TransactionFeeDeficitRecorded',
  bscBroadcasterTransactionFeeDeficitRecorded,
);
