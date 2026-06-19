import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const flipBondUpdated = z.object({ accountId, newBond: numberOrHex });

export const flipBondUpdatedEvent = defineEvent('Flip.BondUpdated', flipBondUpdated);
