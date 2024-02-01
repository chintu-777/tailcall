// Required for the #[global_allocator] proc macro
#![allow(clippy::too_many_arguments)]

use mimalloc::MiMalloc;
use tailcall_cli::CLIError;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn run_blocking() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { tailcall_cli::run().await })
}

fn main() -> anyhow::Result<()> {
    let result = run_blocking();
    match result {
        Ok(_) => {}
        Err(error) => {
            // Ensure all errors are converted to CLIErrors before being printed.
            let cli_error = error.downcast::<CLIError>().unwrap_or_else(|error| {
                let sources = error
                    .source()
                    .map(|error| vec![CLIError::new(error.to_string().as_str())])
                    .unwrap_or_default();

                CLIError::new(&error.to_string()).caused_by(sources)
            });
            eprintln!("{}", cli_error.color(true));
            std::process::exit(exitcode::CONFIG);
        }
    }
    Ok(())
}