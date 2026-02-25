# Chainflip Development Guidelines

## Build/Test/Lint Commands

- Build: `cargo build --release` or `cargo build -p <package>`
- Lint: `cargo check` or `cargo cf-clippy`
- Lint package: `cargo check -p <package>`
- Format: `cargo fmt -- <filename>` or `cargo fmt --all`
- Run all tests: `cargo nextest run`
- Run package tests: `cargo nextest run -p <package>`
- Run single test: `cargo nextest run <test_name>` or `cargo nextest run <module>::<test_name>`
- Show test output: Add `-- --nocapture` to test commands
- Clean build: `cargo clean` or `cargo clean -p <package>`

## Code Style Guidelines

- Follow Substrate code style (github.com/paritytech/substrate/blob/master/docs/STYLE_GUIDE.md)
- Formatting: 100 char line width, hard tabs, vertical trailing commas
- Errors: Use `Err(anyhow!("message"))` at end of functions, `bail!()` for early returns
- PRs: Keep small (<400 lines), organize meaningful commits
- Prioritize readability and maintainability over cleverness
- Commits: Use prefixes `feat:`, `fix:`, `refactor:`, `test:`, `doc:`, `chore:`
- Run localnet with `./localnet/manage.sh` for testing

## Security

- Never expose, log, or commit secrets or keys
- Security is paramount - follow best practices
