import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });

export const bscBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'BscBroadcaster.TransactionFeeDeficitRefused',
  bscBroadcasterTransactionFeeDeficitRefused,
);
