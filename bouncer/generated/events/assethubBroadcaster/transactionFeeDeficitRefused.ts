import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });

export const assethubBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'AssethubBroadcaster.TransactionFeeDeficitRefused',
  assethubBroadcasterTransactionFeeDeficitRefused,
);
