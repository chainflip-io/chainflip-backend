import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });

export const solanaBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'SolanaBroadcaster.TransactionFeeDeficitRefused',
  solanaBroadcasterTransactionFeeDeficitRefused,
);
