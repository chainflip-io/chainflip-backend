import { arbitrumElectionsUnknownElectionEvent } from '../../arbitrumElections/unknownElection';
import { bitcoinElectionsUnknownElectionEvent } from '../../bitcoinElections/unknownElection';
import { bscElectionsUnknownElectionEvent } from '../../bscElections/unknownElection';
import { ethereumElectionsUnknownElectionEvent } from '../../ethereumElections/unknownElection';
import { genericElectionsUnknownElectionEvent } from '../../genericElections/unknownElection';
import { solanaElectionsUnknownElectionEvent } from '../../solanaElections/unknownElection';
import { tronElectionsUnknownElectionEvent } from '../../tronElections/unknownElection';

export const electionsUnknownElectionEvent = {
  Arbitrum: arbitrumElectionsUnknownElectionEvent,
  Bitcoin: bitcoinElectionsUnknownElectionEvent,
  Bsc: bscElectionsUnknownElectionEvent,
  Ethereum: ethereumElectionsUnknownElectionEvent,
  Generic: genericElectionsUnknownElectionEvent,
  Solana: solanaElectionsUnknownElectionEvent,
  Tron: tronElectionsUnknownElectionEvent,
} as const;
