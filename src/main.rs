use actix_web::{get, App, HttpServer, Responder};
use clap::Parser;
use futures::StreamExt;
use lazy_static::lazy_static;
use prometheus::{Encoder, IntGauge, IntCounter};
use tracing_subscriber::EnvFilter;

use near_lake_framework::LakeConfig;
use configs::Opts;

mod configs;

lazy_static! {
    static ref LATEST_BLOCK_HEIGHT: IntGauge = IntGauge::new("pulse_latest_block", "Latest known block height").unwrap();
    static ref BLOCKS_INDEXED: IntCounter = IntCounter::new("pulse_blocks_indexed", "Number of indexed blocks").unwrap();
}

#[tokio::main]
async fn main() -> Result<(), tokio::io::Error> {
    init_tracing();

    let opts: Opts = Opts::parse();
    let http_port = opts.http_port.clone();

    let config: LakeConfig = opts.chain_id.into();
    let stream = near_lake_framework::streamer(config);

    // Register custom metrics to a custom registry.
    prometheus::default_registry()
        .register(Box::new(LATEST_BLOCK_HEIGHT.clone()))
        .unwrap();
    prometheus::default_registry()
        .register(Box::new(BLOCKS_INDEXED.clone()))
        .unwrap();

    tokio::spawn(async move {
        let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
            .map(|streamer_message| handle_streamer_message(streamer_message))
            .buffer_unordered(1usize);

        while let Some(_handle_message) = handlers.next().await {}
    });

    HttpServer::new(|| {
        App::new().service(metrics)
    })
    .bind(("0.0.0.0", http_port))?
    .run()
    .await
    .unwrap();

    Ok(())
}

async fn handle_streamer_message(
    streamer_message: near_lake_framework::near_indexer_primitives::StreamerMessage,
) {
    BLOCKS_INDEXED.inc();
    LATEST_BLOCK_HEIGHT.set(streamer_message.block.header.height.try_into().unwrap());
    eprintln!(
        "{} / shards {}",
        streamer_message.block.header.height,
        streamer_message.shards.len()
    );
}

#[get("/metrics")]
async fn metrics() -> impl Responder {
    let mut buffer = Vec::<u8>::new();
    let encoder = prometheus::TextEncoder::new();
    loop {
        match encoder.encode(&prometheus::gather(), &mut buffer) {
            Ok(_) => break,
            Err(err) => {
                eprintln!("{:?}", err);
            }
        }
    }
    format!("{}", String::from_utf8(buffer.clone()).unwrap())
}

fn init_tracing() {
    let mut env_filter = EnvFilter::new("near_lake_framework=debug");

    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            for directive in rust_log.split(',').filter_map(|s| match s.parse() {
                Ok(directive) => Some(directive),
                Err(err) => {
                    eprintln!("Ignoring directive `{}`: {}", s, err);
                    None
                }
            }) {
                env_filter = env_filter.add_directive(directive);
            }
        }
    }

    tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();
}
