use fedimint_cli::FedimintCli;

#[tokio::main(worker_threads = 1)]
async fn main() -> anyhow::Result<()> {
    FedimintCli::new()?.with_default_modules().run().await;
    Ok(())
}
