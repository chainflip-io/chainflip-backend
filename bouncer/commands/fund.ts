import { fund } from '../shared/fund';
import { Token } from '../shared/utils';

fund(process.argv[2].toUpperCase() as Token, process.argv[3]);