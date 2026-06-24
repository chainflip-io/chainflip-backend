import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinVaultAwaitingGovernanceActivation = z.object({
  newPublicKey: cfChainsBtcAggKey,
});

export const bitcoinVaultAwaitingGovernanceActivationEvent = defineEvent(
  'BitcoinVault.AwaitingGovernanceActivation',
  bitcoinVaultAwaitingGovernanceActivation,
);
