use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, short, default_value = "127.0.0.1:9999")]
    proxy_address: String,

    #[arg(long, short, default_value = "127.0.0.1:6379")]
    backend_addresses: String,

    #[arg(short, long, default_value_t = 1)]
    threads: usize,

    #[arg(short, long, default_value_t = 32)]
    group_size: usize,
}

fn main() {
    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let backend_addresses = args
        .backend_addresses
        .split(',')
        .map(|s| s.to_string())
        .collect();

    let config = libredisproxy::Config {
        proxy_address: args.proxy_address,
        backend_addresses,
        group_sessions: args.group_size,
    };

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(libredisproxy::run_ultra_service(config));
}
