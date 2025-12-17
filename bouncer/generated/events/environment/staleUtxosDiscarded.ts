import { z } from 'zod';
import { cfChainsBtcUtxo } from '../common';

export const environmentStaleUtxosDiscarded = z.object({ utxos: z.array(cfChainsBtcUtxo) });
