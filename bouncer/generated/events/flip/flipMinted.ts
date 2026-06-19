import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const flipFlipMinted = z.object({ to: accountId, amount: numberOrHex });

export const flipFlipMintedEvent = defineEvent('Flip.FlipMinted', flipFlipMinted);
