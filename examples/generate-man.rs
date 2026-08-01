use std::fs::{self, File};

use clap::CommandFactory;
use clap_mangen::Man;
use invert::cli::Cli;

fn main() -> std::io::Result<()> {
    fs::create_dir_all("man")?;
    let mut output = File::create("man/invert.1")?;
    Man::new(Cli::command()).render(&mut output)?;
    Ok(())
}
