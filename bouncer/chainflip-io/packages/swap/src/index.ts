import 'dotenv/config';
import { Server } from 'http';
import start from './processor';
import server from './server';
import { handleExit } from './utils/function';
import logger from './utils/logger';

const PORT =
  Number.parseInt(process.env.SWAPPING_APP_PORT as string, 10) || 8080;

start();

server.listen(
  PORT,
  // eslint-disable-next-line func-names
  function (this: Server) {
    logger.info(`server listening on ${PORT}`, { address: this.address() });

    handleExit(() => this.close());
  },
);

process.on('exit', (code) => {
  logger.info(`process exiting with code "${code}"`);
});
