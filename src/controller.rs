use anyhow::Result;
use futures::prelude::*;
use std::collections::HashMap;

use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::ByteString;
use kube::api::{Api, ListParams};
use kube::Client;
use kube_runtime::controller::{Context, Controller, ReconcilerAction};
use structopt::StructOpt;
use thiserror::Error;

use crate::crd::{check_crd_status, PatchStrategy, TradeSecret};
use crate::duration::Duration;

/// The error type returned by the controller.
#[derive(Error, Debug)]
pub enum ReconcilerError {
    /// The source or destination secret wasn't found.
    #[error("Secret was not found")]
    SecretNotFound,

    /// The source field is missing.
    #[error("Source data field is missing")]
    SourceFieldMissing,

    /// Random errors
    #[error("Unknown error")]
    Unknown,
}

/// The context passed to the controller
struct Ctx {
    client: kube::Client,
    requeue_time: Duration,
}

impl Ctx {
    fn new(client: kube::Client, requeue_time: Duration) -> Self {
        Self {
            client,
            requeue_time,
        }
    }

    fn client(&self) -> &kube::Client {
        &self.client
    }

    fn requeue_action(&self) -> ReconcilerAction {
        ReconcilerAction {
            requeue_after: Some(self.requeue_time.into()),
        }
    }
}

/// The core of the controller.
///
/// Takes a trade secret
/// Performs updates, if necessary.
/// Requeues the trade secret for processing again in 5 minutes
async fn reconcile(
    trade: TradeSecret,
    ctx: Context<Ctx>,
) -> Result<ReconcilerAction, ReconcilerError> {
    // eprintln!("TradeSecret: {:?}", trade);

    // The namespace the TradeSecret is in is the namespace the secrets are in
    let namespace: &str = &trade
        .metadata
        .namespace
        .as_ref()
        .expect("TradeSecrets should always be namespaced");

    eprintln!("Updating {}.{}", namespace, trade.metadata.name.unwrap());

    let secrets: Api<Secret> = Api::namespaced(ctx.get_ref().client().clone(), namespace);

    let source_secret = secrets
        .get(&trade.spec.source)
        .await
        .map_err(|_| ReconcilerError::SecretNotFound)?;

    let dest_secret = secrets
        .get(&trade.spec.destination)
        .await
        .map_err(|_| ReconcilerError::SecretNotFound)?;

    match trade.spec.strategy {
        // Simple copy of a field from source to dest
        PatchStrategy::Copy { ref items } => {
            // Compile a hashmap of all the source values we care about.
            // If any are missing, raise an error
            let source_values: HashMap<String, ByteString> = items
                .iter()
                .map(|item| {
                    source_secret
                        .data
                        .as_ref()
                        .and_then(|data| {
                            data.get(&item.source)
                                .map(|value| (item.source.clone(), value.clone()))
                        })
                        .ok_or(ReconcilerError::SourceFieldMissing)
                })
                .collect::<Result<HashMap<_, _>, ReconcilerError>>()?;

            // Compile a hashmap of Options of all the dest values we care about.
            let dest_values: HashMap<String, Option<ByteString>> = items
                .iter()
                .map(|item| {
                    dest_secret
                        .data
                        .as_ref()
                        .and_then(|data| {
                            data.get(&item.destination)
                                .map(|value| (item.destination.clone(), Some(value.clone())))
                        })
                        .unwrap_or_else(|| (item.destination.clone(), None))
                })
                .collect();

            // Compile a hashmap of all the changes that need to be applied
            let updates: HashMap<&str, &ByteString> = items
                .iter()
                .filter_map(|item| {
                    let source_value = source_values
                        .get(&item.source)
                        .expect("source key should exist");

                    let dest_value = dest_values
                        .get(&item.destination)
                        .expect("dest key should atleast be None")
                        .as_ref();

                    if dest_value.is_none() || *source_value != *dest_value.unwrap() {
                        Some((item.destination.as_ref(), source_value))
                    } else {
                        None
                    }
                })
                .collect();

            // If there's no changes to apply, we're done
            if updates.is_empty() {
                eprintln!("Destination already matches source. No updates needed.");
                return Ok(ctx.get_ref().requeue_action());
            }

            // Create the patch from the changes
            let patch = serde_json::to_vec(&serde_json::json!({ "data": updates }))
                .map_err(|_| ReconcilerError::Unknown)?;

            eprintln!(
                "Updating secret '{}': {}",
                trade.spec.destination,
                std::str::from_utf8(&patch).map_err(|_| ReconcilerError::Unknown)?,
            );

            // Apply the patch in strategic merge mode
            secrets
                .patch(&trade.spec.destination, &Default::default(), patch)
                .await
                .map_err(|e| {
                    eprintln!("Error {}: {:?}", e, e);
                    ReconcilerError::Unknown
                })?;
        }
    };

    Ok(ctx.get_ref().requeue_action())
}

/// The controller error handler.
///
/// Don't do anything.  Just requeue the trade secrets.
fn error_policy(_error: &ReconcilerError, ctx: Context<Ctx>) -> ReconcilerAction {
    ctx.get_ref().requeue_action()
}

/// Controller creation
async fn controller(client: Client, requeue_time: Duration) -> Result<()> {
    let trades = Api::<TradeSecret>::all(client.clone());
    let context = Context::new(Ctx::new(client.clone(), requeue_time));

    Controller::new(trades, ListParams::default())
        .run(reconcile, error_policy, context)
        // TODO: This is from the example.  This should be done differently.
        .for_each(|res| async move {
            match res {
                Ok(_) => {}
                Err(e) => eprintln!("reconcile failed: {:?}", e),
            }
        })
        .await;

    Ok(())
}

/// Run the controller
pub(crate) async fn run_controller(client: kube::Client, requeue_time: Duration) -> Result<()> {
    check_crd_status(client.clone()).await?;
    controller(client.clone(), requeue_time).await
}

/// controller related commands
#[derive(StructOpt, Debug, Clone)]
pub(crate) enum ControllerCommand {
    /// Run the controller
    Run {
        #[structopt(long, default_value = "5m", env = "TS_REQUEUE_TIME")]
        requeue_time: Duration,
    },
}

pub(crate) async fn run_command(command: ControllerCommand) -> Result<()> {
    let client = kube::Client::try_default().await?;

    match command {
        ControllerCommand::Run { requeue_time, .. } => run_controller(client, requeue_time).await,
    }
}
