import { arbitrumElectionsAllVotesNotClearedEvent } from '../../arbitrumElections/allVotesNotCleared';
import { bitcoinElectionsAllVotesNotClearedEvent } from '../../bitcoinElections/allVotesNotCleared';
import { bscElectionsAllVotesNotClearedEvent } from '../../bscElections/allVotesNotCleared';
import { ethereumElectionsAllVotesNotClearedEvent } from '../../ethereumElections/allVotesNotCleared';
import { genericElectionsAllVotesNotClearedEvent } from '../../genericElections/allVotesNotCleared';
import { solanaElectionsAllVotesNotClearedEvent } from '../../solanaElections/allVotesNotCleared';
import { tronElectionsAllVotesNotClearedEvent } from '../../tronElections/allVotesNotCleared';

export const electionsAllVotesNotClearedEvent = {
  Arbitrum: arbitrumElectionsAllVotesNotClearedEvent,
  Bitcoin: bitcoinElectionsAllVotesNotClearedEvent,
  Bsc: bscElectionsAllVotesNotClearedEvent,
  Ethereum: ethereumElectionsAllVotesNotClearedEvent,
  Generic: genericElectionsAllVotesNotClearedEvent,
  Solana: solanaElectionsAllVotesNotClearedEvent,
  Tron: tronElectionsAllVotesNotClearedEvent,
} as const;
