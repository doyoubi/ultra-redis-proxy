build:
	cargo build

run:
	RUST_LOG=libredisproxy=info,ultra=info target/debug/ultra-redis-proxy

