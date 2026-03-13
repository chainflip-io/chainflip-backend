import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const bscVaultAwaitingGovernanceActivation = z.object({ newPublicKey: cfChainsEvmAggKey });
