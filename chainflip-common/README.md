A library for common chainflip logic which will be used by the backend, state chain or quoter.

# Usage

```toml
[dependencies]
chainflip-common = { default-features = false, version =  '0.0.1' }

[features]
default = ['std']
std = [
    'chainflip-common/std',
]
```