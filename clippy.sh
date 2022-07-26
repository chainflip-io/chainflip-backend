cat <<\EOF
Running: cargo clippy --all-targets -- -D warnings -A clippy::boxed_local
EOF

set -e
cargo clippy --all-targets -- -D warnings -A clippy::boxed_local
