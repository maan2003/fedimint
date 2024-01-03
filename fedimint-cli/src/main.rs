use fedimint_cli::FedimintCli;

#[tokio::main(worker_threads = 1)]
async fn main() -> anyhow::Result<()> {
    FedimintCli::new(env!("FEDIMINT_BUILD_CODE_VERSION"))?
        .with_default_modules()
        .run()
        .await;
    Ok(())
}
