// Serves the WebAuthn PoC page on http://localhost (a secure context, so WebAuthn is available)
// and writes recorded ceremonies to ../fixtures so tests can consume them.
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';

const port = Number(process.env.PORT ?? 8765);
const fixturesDir = join(import.meta.dir, '..', 'fixtures');
mkdirSync(fixturesDir, { recursive: true });

Bun.serve({
  port,
  hostname: 'localhost',
  async fetch(req) {
    const url = new URL(req.url);
    if (req.method === 'GET' && url.pathname === '/') {
      return new Response(Bun.file(join(import.meta.dir, 'index.html')));
    }
    if (req.method === 'GET' && url.pathname === '/fixtures/challenges.json') {
      return new Response(Bun.file(join(fixturesDir, 'challenges.json')));
    }
    const save = url.pathname.match(/^\/fixtures\/([a-z0-9_-]+)$/);
    if (req.method === 'POST' && save) {
      const path = join(fixturesDir, `${save[1]}.json`);
      await Bun.write(path, JSON.stringify(await req.json(), null, 2) + '\n');
      return new Response(path);
    }
    return new Response('not found', { status: 404 });
  },
});

console.log(`WebAuthn PoC page: http://localhost:${port}`);
