import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});

export const arbitrumBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'ArbitrumBroadcaster.TransactionFeeDeficitRecorded',
  arbitrumBroadcasterTransactionFeeDeficitRecorded,
);
