import { z } from 'zod';
import { cfChainsBtcUtxo } from '../common';

export const bitcoinIngressEgressTransactionRejectionFailed = z.object({ txId: cfChainsBtcUtxo });
