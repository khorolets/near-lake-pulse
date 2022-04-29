use clap::Parser;

/// NEAR Lake Pulse
/// Provides metrics data to keep an eye for NEAR Lake data
#[derive(Parser, Debug)]
#[clap(
    version,
    author,
    about,
    disable_help_subcommand(true),
    propagate_version(true),
    next_line_help(true)
)]
pub(crate) struct Opts {
    #[clap(long, short, default_value = "3030")]
    pub http_port: u16,
    #[clap(long)]
    pub telegram_token: Option<String>,
    #[clap(long)]
    pub chat_id: Vec<String>,
    #[clap(subcommand)]
    pub chain_id: ChainId,
}

#[derive(Parser, Debug)]
pub(crate) enum ChainId {
    Mainnet(RunArgs),
    Testnet(RunArgs),
    Localnet(RunArgs),
}

#[derive(Parser, Debug)]
pub(crate) struct RunArgs {
    /// Block heigh to start watching from
    #[clap(short, long)]
    pub block_height: u64,
}

impl From<ChainId> for near_lake_framework::LakeConfig {
    fn from(chain: ChainId) -> near_lake_framework::LakeConfig {
        match chain {
            ChainId::Mainnet(args) => near_lake_framework::LakeConfig {
                s3_endpoint: None,
                s3_bucket_name: "near-lake-data-mainnet".to_string(),
                s3_region_name: "eu-central-1".to_string(),
                start_block_height: args.block_height,
            },
            ChainId::Testnet(args) => near_lake_framework::LakeConfig {
                s3_endpoint: None,
                s3_bucket_name: "near-lake-data-testnet".to_string(),
                s3_region_name: "eu-central-1".to_string(),
                start_block_height: args.block_height,
            },
            ChainId::Localnet(args) => near_lake_framework::LakeConfig {
                s3_endpoint: None,
                s3_bucket_name: "near-lake-data-localnet".to_string(),
                s3_region_name: "eu-central-1".to_string(),
                start_block_height: args.block_height,
            },
        }
    }
}
