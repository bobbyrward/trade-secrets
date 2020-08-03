//! trade-secrets operator
//!
//! Patches a secret with the contents of another secret
//!
use anyhow::Result;
use structopt::StructOpt;

mod controller;
mod crd;
mod duration;

#[derive(StructOpt, Debug, Clone)]
enum Command {
    Crd {
        #[structopt(flatten)]
        command: crd::CrdCommand,
    },
    Controller {
        #[structopt(flatten)]
        command: controller::ControllerCommand,
    },
}

#[derive(StructOpt, Debug, Clone)]
struct Args {
    #[structopt(flatten)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::from_args();

    match args.command {
        Command::Crd { command, .. } => {
            crd::run_command(command).await?;
        }
        Command::Controller { command, .. } => {
            controller::run_command(command).await?;
        }
    };

    Ok(())
}
