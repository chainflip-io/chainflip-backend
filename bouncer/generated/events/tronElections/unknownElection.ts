import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple5ImplsCompositeElectionIdentifierExtra,
} from '../common';

export const tronElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple5ImplsCompositeElectionIdentifierExtra,
]);
