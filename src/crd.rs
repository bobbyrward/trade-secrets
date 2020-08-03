use anyhow::{anyhow, Context, Result};
use apiexts::CustomResourceDefinition;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1beta1 as apiexts;
use kube::api::Api;
use kube_derive::CustomResource;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;

const CRD_NAME: &str = "tradesecrets.secrets.ohnozombi.es";
const CRD_VERSION: &str = "v1alpha1";

/// An individual copy operation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PatchCopyItem {
    pub source: String,
    pub destination: String,
}

/// A patch strategy, mostly for future expansion
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub(crate) enum PatchStrategy {
    #[serde(rename = "copy")]
    Copy { items: Vec<PatchCopyItem> },
}

/// The TradeSecret type
#[derive(CustomResource, Debug, Clone, Deserialize, Serialize)]
#[kube(group = "secrets.ohnozombi.es", version = "v1alpha1", namespaced)]
#[kube(shortname = "trades")]
// Use v1beta1 so it doesn't require a schema, which isn't supported by kube
#[kube(apiextensions = "v1beta1")]
pub(crate) struct TradeSecretSpec {
    pub source: String,
    pub destination: String,
    pub strategy: PatchStrategy,
}

/// Checks that the CRD exists in the cluster
pub(crate) async fn check_crd_status(client: kube::Client) -> Result<()> {
    let crds = Api::<CustomResourceDefinition>::all(client.clone());
    let ts_crd = crds
        .get(CRD_NAME)
        .await
        .with_context(|| format!("Could not find the crd: {}", CRD_NAME))?;

    let versions = ts_crd
        .spec
        .versions
        .ok_or_else(|| anyhow!("The CRD is missing the version field."))?;

    if versions.len() != 1 {
        return Err(anyhow!("Only expected one version in the CRD."));
    }

    if versions[0].name != CRD_VERSION {
        return Err(anyhow!("CRD is not the expected version: {}", CRD_VERSION));
    }

    Ok(())
}

/// Crd related commands
#[derive(StructOpt, Debug, Clone)]
pub(crate) enum CrdCommand {
    /// Print CRD definition
    Export {},
    /// Check that the expected CRD is installed
    Check {},
}

/// Print CRD definition
pub(crate) async fn print_crd() -> Result<()> {
    let crd = TradeSecret::crd();
    print!("{}", serde_yaml::to_string(&crd)?);
    Ok(())
}

pub(crate) async fn run_command(command: CrdCommand) -> Result<()> {
    match command {
        CrdCommand::Export { .. } => print_crd().await,
        CrdCommand::Check { .. } => {
            let client = kube::Client::try_default().await?;
            check_crd_status(client).await?;
            println!("CRD is installed.");
            Ok(())
        }
    }
}
