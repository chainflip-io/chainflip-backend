import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const governanceProposed = z.number();

export const governanceProposedEvent = defineEvent('Governance.Proposed', governanceProposed);
