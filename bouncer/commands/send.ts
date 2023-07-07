import { send } from '../shared/send';
import { Token } from '../shared/utils';

send(process.argv[2].toUpperCase() as Token, process.argv[3]);