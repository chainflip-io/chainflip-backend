import { z } from 'zod';
import { cfChainsBtcUtxoSelectionConsolidationParameters } from '../common';

export const environmentUtxoConsolidationParametersUpdated = z.object({
  params: cfChainsBtcUtxoSelectionConsolidationParameters,
});
