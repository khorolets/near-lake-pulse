# near-lake-pulse

It is a simple NEAR Lake indexer that just prints the block heights and exposes some Prometheus-friendly metrics:

- latest indexed block height
- number of blocks indexed

It is supposed to be used for healthcheck and check that the data we've reindexed to the AWS S3 buckets is consistent and doesn't have
any gaps.

## Usage

**Don't forget** you need to have `~/.aws/credentials` with your AWS credentials

Basic:

`./target/release/near-lake-pulse <chain_id> --block-height <BLOCK_HEIGHT>`


Mainnet:

`./target/release/near-lake-pulse mainnet --block-height 61941713`

Testnet

`./target/release/near-lake-pulse mainnet --block-height 85635752`

