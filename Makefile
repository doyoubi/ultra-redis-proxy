build:
	cargo build -r

build-debug:
	cargo build

run:
	RUST_LOG=libredisproxy=info,ultra=info target/release/ultra-redis-proxy

run-debug:
	RUST_LOG=libredisproxy=info,ultra=info target/debug/ultra-redis-proxy

