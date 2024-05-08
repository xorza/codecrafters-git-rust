#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use clap::{Arg, ArgMatches, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match get_matches().subcommand() {
        Some(("init", _decode_matches)) => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory")
        }
        _ => { eprintln!("Invalid command, use --help."); }
    }
    
    Ok(())
}

fn get_matches() -> ArgMatches {
    Command::new("Rust Git")
        .version("0.1.0")
        .author("xxorza")
        .about("A simple git implementation in Rust")
        .subcommand(
            Command::new("init")
                .about("Initialize a new git repository")
            // .arg(
            //     Arg::new("encoded_value")
            //     .required(true)
            // ),
        )
        .get_matches()
}
