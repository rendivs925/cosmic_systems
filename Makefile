.PHONY: fmt check clippy test run run-craft run-rocket run-gyro build-wasm

fmt:
	cargo fmt

check:
	cargo check

clippy:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

run:
	cargo run

run-craft:
	cargo run -- craft

run-rocket:
	cargo run -- rocket

run-gyro:
	cargo run -- gyro

build-wasm:
	cargo build --lib --target wasm32-unknown-unknown --no-default-features
