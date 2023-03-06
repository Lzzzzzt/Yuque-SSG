#![feature(result_option_inspect)]

use log::error;
use yuque_ssg::{config::Config, generator::Generator, log::init_logger};

use std::{
    error::Error,
    os::unix::process::CommandExt,
    process::{exit, Command},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logger();

    let config = Config::read_config("config.yml")
        .and_then(|config| config.check_env())
        .and_then(|config| config.generate_config_js())
        .inspect_err(|e| {
            error!("{}", e);
            exit(1)
        })?;

    let generator: Generator = config.into();
    generator.generate().await?;

    Command::new("npm")
        .args(["run", "docs:dev", "--", "--host", "0.0.0.0"])
        .exec();

    Ok(())
}
