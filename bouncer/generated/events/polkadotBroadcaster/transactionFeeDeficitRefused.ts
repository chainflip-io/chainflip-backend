import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });

export const polkadotBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'PolkadotBroadcaster.TransactionFeeDeficitRefused',
  polkadotBroadcasterTransactionFeeDeficitRefused,
);
