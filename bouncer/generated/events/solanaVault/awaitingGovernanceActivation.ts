import { z } from 'zod';
import { hexString } from '../common';

export const solanaVaultAwaitingGovernanceActivation = z.object({ newPublicKey: hexString });
