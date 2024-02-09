import 'dotenv/config';
import { Server, createServer } from 'http';
import env from './config/env';
import start from './processor';
import server from './server';
import { handleExit } from './utils/function';
import logger from './utils/logger';

if (env.START_PROCESSOR) {
  start();

  if (!env.START_HTTP_SERVICE) {
    const healthServer = createServer((req, res) => {
      res.end('i am still alive\n');
    }).listen(env.SWAPPING_APP_PORT, () => {
      logger.info(`server listening on ${env.SWAPPING_APP_PORT}`, {
        address: healthServer.address(),
      });

      handleExit(() => healthServer.close());
    });
  }
}

if (env.START_HTTP_SERVICE) {
  server.listen(
    env.SWAPPING_APP_PORT,
    // eslint-disable-next-line func-names
    function (this: Server) {
      logger.info(`server listening on ${env.SWAPPING_APP_PORT}`, {
        address: this.address(),
      });

      handleExit(() => this.close());
    },
  );
}

if (!env.START_HTTP_SERVICE && !env.START_PROCESSOR) {
  logger.error('no services started');
  process.exit(1);
}

process.on('exit', (code) => {
  logger.info(`process exiting with code "${code}"`);
});
