import { z } from 'zod';
import { cfChainsBtcUtxoSelectionConsolidationParameters } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentUtxoConsolidationParametersUpdated = z.object({
  params: cfChainsBtcUtxoSelectionConsolidationParameters,
});

export const environmentUtxoConsolidationParametersUpdatedEvent = defineEvent(
  'Environment.UtxoConsolidationParametersUpdated',
  environmentUtxoConsolidationParametersUpdated,
);
