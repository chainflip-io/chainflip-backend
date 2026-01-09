import { z } from 'zod';
import { hexString } from '../common';

export const assethubVaultAwaitingGovernanceActivation = z.object({ newPublicKey: hexString });
