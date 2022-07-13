cat <<\EOF
Running: cargo clippy --all-targets -- -D warnings -A clippy::boxed_local -A clippy::nonstandard_macro_braces
EOF

set -e
cargo clippy --all-targets -- -D warnings -A clippy::boxed_local -A clippy::nonstandard_macro_braces
