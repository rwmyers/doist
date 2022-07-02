use color_eyre::{eyre::Context, Result};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};

use crate::api::{self, rest::Gateway};

#[derive(clap::Parser, Debug, Deserialize, Serialize)]
pub struct Params {
    /// The Task ID as provided from the todoist API. Use `list` to find out what ID your task has.
    id: api::rest::TaskID,
}

pub async fn close(params: Params, gw: &Gateway) -> Result<()> {
    gw.close(params.id).await.context("unable to close task")?;
    println!("closed task {}", params.id.bright_red());
    Ok(())
}
