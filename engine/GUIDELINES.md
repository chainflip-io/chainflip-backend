# Developer Guidelines (Chainflip Engine)

This document outlines some of the widely agreed upon code style conventions for the Chainflip Engine.

## Errors

### Anyhow Errors
- When returning at the end of a function, use `Err(anyhow!("message here"))`
- When returning early use `bail!("message here")`