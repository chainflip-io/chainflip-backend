import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});

export const solanaBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'SolanaBroadcaster.TransactionFeeDeficitRecorded',
  solanaBroadcasterTransactionFeeDeficitRecorded,
);
