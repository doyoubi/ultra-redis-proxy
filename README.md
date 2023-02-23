# ultra-redis-proxy
Probably the fastest redis proxy.

This project is only for exploring high performance redis proxy implementation.
Currently, it's just a trivial implementation without enough error handling.

## Build
```shell
cargo build --release
```

## Run
Usage:
```
Usage: ultra-redis-proxy [OPTIONS]

Options:
  -p, --proxy-address <PROXY_ADDRESS>          [default: 127.0.0.1:9999]
  -b, --backend-addresses <BACKEND_ADDRESSES>  [default: 127.0.0.1:6379]
  -t, --threads <THREADS>                      [default: 1]
  -g, --group-size <GROUP_SIZE>                [default: 32]
  -h, --help                                   Print help
  -V, --version                                Print version
```

```shell
RUST_LOG=libredisproxy=info,ultra=info target/release/ultra-redis-proxy \
    --threads 1 \
    --backend-addresses 'localhost:6379'

# Or multiple backends with more threads
RUST_LOG=libredisproxy=info,ultra=info target/release/ultra-redis-proxy \
    --threads 4 \
    --backend-addresses 'localhost:6379,localhost:6380'
```
