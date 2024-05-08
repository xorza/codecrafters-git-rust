extern crate core;

use std::fs;
use std::io::{BufRead, Read};

use anyhow::anyhow;
use clap::{Arg, ArgMatches, Command};
use flate2::read::ZlibDecoder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match get_matches().subcommand() {
        Some(("init", _init_matches)) => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n")?;
            println!("Initialized git directory")
        }
        Some(("cat-file", cat_file_matches)) => {
            let blob_sha = cat_file_matches
                .get_one::<String>("blob_sha")
                .expect("blob_sha is required");
            if blob_sha.len() != 40 {
                eprintln!("Invalid blob SHA: {}", blob_sha);
                return Err(anyhow!("Invalid blob SHA: {}", blob_sha));
            }
            let filename = format!(".git/objects/{}/{}", &blob_sha[..2], &blob_sha[2..]);
            let file = fs::File::open(filename)?;
            let decoder = ZlibDecoder::new(file);
            let mut reader = std::io::BufReader::new(decoder);

            let mut buf = Vec::new();
            reader.read_until(0, &mut buf)?;

            let text = std::str::from_utf8(&buf[..buf.len() - 1])?;
            let text = text.strip_prefix("blob ")
                .ok_or(anyhow!("Invalid blob"))?;
            let size: usize = text.parse()?;
            buf.resize(size, 0);
            reader.read_exact(&mut buf)?;

            let content = std::str::from_utf8(&buf)?;
            print!("{}", content);
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
        )
        .subcommand(
            Command::new("cat-file")
                .about("Prints the contents of a git object")
                .arg(
                    Arg::new("blob_sha")
                        .short('p')
                        .required(true)
                        .value_name("BLOB_SHA")
                        .help("The SHA of the blob to print"),
                ),
        )
        .get_matches()
}
