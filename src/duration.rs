/// This is overkill.
use std::str::FromStr;

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use regex::Regex;

/// A duration parseable by structopt
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Duration(tokio::time::Duration);

impl From<Duration> for tokio::time::Duration {
    fn from(source: Duration) -> Self {
        source.0
    }
}

impl FromStr for Duration {
    type Err = anyhow::Error;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref DURATION_RE: Regex =
                Regex::new(r"^(?P<value>\d+)(?P<unit>[smh])?$").unwrap();
        }

        let captures = DURATION_RE
            .captures(source.trim())
            .ok_or_else(|| anyhow!("Invalid duration: {}", source))?;

        let multiplier = match captures.name("unit").map(|c| c.as_str()).unwrap_or("s") {
            "s" => 1u64,
            "m" => 60u64,
            "h" => 3600u64,
            _ => unreachable!(),
        };

        let value = captures.name("value").unwrap().as_str().parse::<u64>()?;

        Ok(Self(tokio::time::Duration::from_secs(value * multiplier)))
    }
}
