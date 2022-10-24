use std::sync::Arc;

use actix_web::{get, App, HttpServer, Responder};
use clap::Parser;
use futures::StreamExt;
use lazy_static::lazy_static;
use prometheus::{Encoder, Gauge, IntCounter, IntGauge};
use teloxide::{
    payloads::SendMessageSetters,
    requests::{Request, Requester},
    types::ParseMode,
    Bot,
};
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use configs::Opts;
use near_lake_framework::LakeConfig;

mod configs;

lazy_static! {
    static ref LATEST_BLOCK_HEIGHT: IntGauge =
        IntGauge::new("pulse_latest_block", "Latest known block height").unwrap();
    static ref BLOCKS_INDEXED: IntCounter =
        IntCounter::new("pulse_blocks_indexed", "Number of indexed blocks").unwrap();
    static ref BPS: Gauge = Gauge::new("pulse_bps", "Blocks per second").unwrap();
}

#[derive(Debug, Clone)]
struct Stats {
    pub blocks_processed_count: u64,
    pub last_processed_block_height: u64,
    pub bps: f64,
}

impl Stats {
    pub fn new() -> Self {
        Self {
            blocks_processed_count: 0,
            last_processed_block_height: 0,
            bps: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum State {
    Alerting,
    Operating,
}

#[tokio::main]
async fn main() -> Result<(), tokio::io::Error> {
    init_tracing();

    let opts: Opts = Opts::parse();
    let telegram_token = opts.telegram_token.clone();
    let chat_ids = opts.chat_id.clone();
    let http_port = opts.http_port;
    let stats_interval_sec = opts.stats_interval_sec;

    let config_string = format!("Chain_id: {}", opts.chain_id());
    let config: LakeConfig = opts.chain_id.into();
    let (_, stream) = near_lake_framework::streamer(config);

    // Register custom metrics to a custom registry.
    prometheus::default_registry()
        .register(Box::new(LATEST_BLOCK_HEIGHT.clone()))
        .unwrap();
    prometheus::default_registry()
        .register(Box::new(BLOCKS_INDEXED.clone()))
        .unwrap();
    prometheus::default_registry()
        .register(Box::new(BPS.clone()))
        .unwrap();

    let stats: Arc<Mutex<Stats>> = Arc::new(Mutex::new(Stats::new()));
    if let Some(token) = telegram_token {
        if !chat_ids.is_empty() {
            let bot = Bot::new(token);

            tokio::spawn(stats_watcher(
                Arc::clone(&stats),
                bot,
                config_string,
                chat_ids,
                stats_interval_sec,
            ));
        }
    }

    tokio::spawn(async move {
        let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
            .map(|streamer_message| handle_streamer_message(streamer_message, Arc::clone(&stats)))
            .buffer_unordered(1usize);

        while let Some(_handle_message) = handlers.next().await {}
    });

    HttpServer::new(|| App::new().service(metrics))
        .bind(("0.0.0.0", http_port))?
        .run()
        .await
        .unwrap();

    Ok(())
}

async fn handle_streamer_message(
    streamer_message: near_lake_framework::near_indexer_primitives::StreamerMessage,
    stats: Arc<Mutex<Stats>>,
) {
    BLOCKS_INDEXED.inc();
    LATEST_BLOCK_HEIGHT.set(streamer_message.block.header.height.try_into().unwrap());
    let mut stats_lock = stats.lock().await;
    BPS.set(stats_lock.bps);
    stats_lock.blocks_processed_count += 1;
    stats_lock.last_processed_block_height = streamer_message.block.header.height;
    drop(stats_lock);
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
    String::from_utf8(buffer.clone()).unwrap()
}

async fn stats_watcher(
    stats: Arc<Mutex<Stats>>,
    bot: Bot,
    config_string: String,
    chat_ids: Vec<String>,
    interval_secs: u64,
) {
    let mut prev_blocks_processed_count: u64 = 0;
    let mut prev_state: State = State::Operating;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        let mut stats_lock = stats.lock().await;

        let block_processing_speed: f64 = ((stats_lock.blocks_processed_count
            - prev_blocks_processed_count) as f64)
            / (interval_secs as f64);
        stats_lock.bps = block_processing_speed;
        prev_blocks_processed_count = stats_lock.blocks_processed_count;
        drop(stats_lock);

        match prev_state {
            State::Alerting => {
                if block_processing_speed > 0.0 {
                    prev_state = State::Operating;
                    for chat_id in chat_ids.iter() {
                        bot.send_message(
                            chat_id.to_string(),
                            format!(
                                "<b>Resolved</b> {}\n BPS is {}",
                                &config_string, block_processing_speed,
                            ),
                        )
                        // Optional parameters can be supplied by calling setters
                        .parse_mode(ParseMode::Html)
                        // To send request to telegram you need to call `.send()` and await the resulting future
                        .send()
                        .await
                        .unwrap();
                    }
                }
            }
            _ => {
                if block_processing_speed <= 0.0 {
                    prev_state = State::Alerting;
                    for chat_id in chat_ids.iter() {
                        bot.send_message(
                            chat_id.to_string(),
                            format!(
                                "<b>Alert!</b> BPS dropped to {}\n{}",
                                block_processing_speed, &config_string,
                            ),
                        )
                        // Optional parameters can be supplied by calling setters
                        .parse_mode(ParseMode::Html)
                        // To send request to telegram you need to call `.send()` and await the resulting future
                        .send()
                        .await
                        .unwrap();
                    }
                }
            }
        };
    }
}

fn init_tracing() {
    let mut env_filter = EnvFilter::new("near_lake_framework=info");

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
