# Bouncer (TypeScript)

The `bouncer/` directory contains end-to-end tests and operational scripts.

## Key Patterns

- **Use `ChainflipIO`** (`bouncer/shared/utils/chainflip_io.ts`) for all state chain interactions. It tracks block heights for event ordering and provides type-safe extrinsic submission and event waiting. Prefer extending `ChainflipIO` over writing ad-hoc queries.
- **Generated event types** live in `bouncer/generated/events/` with zod schemas for type-safe event parsing.
- **Use the indexer** (`bouncer/shared/utils/indexer.ts`) for querying events by block range, not direct RPC polling.
- Tests use `vitest` with `concurrentTest` / `serialTest` helpers for parallel/serial execution.
- Test files go in `bouncer/tests/`, shared utilities in `bouncer/shared/`, CLI commands in `bouncer/commands/`.

For running tests, managing the localnet, regenerating event schemas, or any related lifecycle task, use the `bouncer` skill.
