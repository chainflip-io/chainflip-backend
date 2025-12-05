import { z } from 'zod';
import { hexString } from '../common';

export const polkadotVaultAwaitingGovernanceActivation = z.object({ newPublicKey: hexString });
