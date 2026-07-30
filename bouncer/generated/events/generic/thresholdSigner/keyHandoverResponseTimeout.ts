import { bitcoinThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../bitcoinThresholdSigner/keyHandoverResponseTimeout';
import { evmThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../evmThresholdSigner/keyHandoverResponseTimeout';
import { polkadotThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../polkadotThresholdSigner/keyHandoverResponseTimeout';
import { solanaThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../solanaThresholdSigner/keyHandoverResponseTimeout';

export const thresholdSignerKeyHandoverResponseTimeoutEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverResponseTimeoutEvent,
  Evm: evmThresholdSignerKeyHandoverResponseTimeoutEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverResponseTimeoutEvent,
  Solana: solanaThresholdSignerKeyHandoverResponseTimeoutEvent,
} as const;
