cat <<\EOF
Running: cargo clippy --all-targets -- -D warnings
EOF

set -e
cargo clippy --all-targets -- -D warnings
