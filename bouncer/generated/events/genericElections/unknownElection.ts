import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple1ImplsCompositeElectionIdentifierExtra,
} from '../common';

export const genericElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple1ImplsCompositeElectionIdentifierExtra,
]);
