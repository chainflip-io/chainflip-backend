# This Dockfile provides the base image to perform all tasks
# related to our Rust projects. Our CI needs a properly configured
# environment so we can guarantee consistancy between projects.
FROM gitpod/workspace-full

# Download and set nightly as the default Rust compiler
RUN rustup default nightly-2021-03-24 \
    && rustup target add wasm32-unknown-unknown --toolchain nightly-2021-03-24 
