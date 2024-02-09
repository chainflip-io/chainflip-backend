import 'dotenv/config';
import { exec } from 'child_process';
import { promisify } from 'util';

const { DB_USER, DB_PASS, DB_PORT, DB_NAME } = process.env;
process.env.DATABASE_URL = `postgresql://${DB_USER}:${DB_PASS}@127.0.0.1:${DB_PORT}/${DB_NAME}_test?schema=public`;
process.env.INGEST_GATEWAY_URL = 'https://ingest-gateway.test';

const execAsync = promisify(exec);

export default async () => {
  await execAsync('pnpm prisma migrate reset --force --skip-generate');
};
