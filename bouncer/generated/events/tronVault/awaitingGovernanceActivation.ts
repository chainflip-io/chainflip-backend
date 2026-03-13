import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const tronVaultAwaitingGovernanceActivation = z.object({ newPublicKey: cfChainsEvmAggKey });
