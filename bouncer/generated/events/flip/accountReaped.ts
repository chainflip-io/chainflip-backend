import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const flipAccountReaped = z.object({ who: accountId, dustBurned: numberOrHex });

export const flipAccountReapedEvent = defineEvent('Flip.AccountReaped', flipAccountReaped);
