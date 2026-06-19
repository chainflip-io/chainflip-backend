import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const witnesserReportedWitnessingFailures = z.object({
  callHash: hexString,
  blockNumber: z.number(),
  accounts: z.array(accountId),
});

export const witnesserReportedWitnessingFailuresEvent = defineEvent(
  'Witnesser.ReportedWitnessingFailures',
  witnesserReportedWitnessingFailures,
);
