import { arbitrumElectionsElectoralEventEvent } from '../../arbitrumElections/electoralEvent';
import { bitcoinElectionsElectoralEventEvent } from '../../bitcoinElections/electoralEvent';
import { bscElectionsElectoralEventEvent } from '../../bscElections/electoralEvent';
import { ethereumElectionsElectoralEventEvent } from '../../ethereumElections/electoralEvent';
import { genericElectionsElectoralEventEvent } from '../../genericElections/electoralEvent';
import { solanaElectionsElectoralEventEvent } from '../../solanaElections/electoralEvent';
import { tronElectionsElectoralEventEvent } from '../../tronElections/electoralEvent';

export const electionsElectoralEventEvent = {
  Arbitrum: arbitrumElectionsElectoralEventEvent,
  Bitcoin: bitcoinElectionsElectoralEventEvent,
  Bsc: bscElectionsElectoralEventEvent,
  Ethereum: ethereumElectionsElectoralEventEvent,
  Generic: genericElectionsElectoralEventEvent,
  Solana: solanaElectionsElectoralEventEvent,
  Tron: tronElectionsElectoralEventEvent,
} as const;
