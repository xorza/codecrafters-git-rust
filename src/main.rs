extern crate core;

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};

use anyhow::anyhow;
use bytes::{BufMut, BytesMut};
use clap::{Arg, ArgAction, ArgMatches, Command};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sha1::Digest;

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
            let filename = filename_from_sha(&blob_sha)?;
            let file = fs::File::open(filename)?;
            let decoder = ZlibDecoder::new(file);
            let mut reader = BufReader::new(decoder);

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
        Some(("hash-object", hash_object_matches)) => {
            let filename = hash_object_matches
                .get_one::<String>("file")
                .expect("file is required");
            let should_write = hash_object_matches.get_flag("write");
            
            let mut input_file = fs::File::open(&filename)?;
            let size = input_file.metadata()?.len();
            
            let mut buf = BytesMut::new();
            buf.write_str("blob ")?;
            buf.write_str(&size.to_string())?;
            buf.put_u8(0);
            let start_content = buf.len();
            buf.resize(start_content + size as usize, 0);
            
            input_file.read_exact(&mut buf[start_content..])?;
            
            let content = String::from_utf8_lossy(&buf[start_content..]);
            dbg!(content);
            
            let mut hasher = sha1::Sha1::new();
            hasher.update(&buf);
            let blob_sha = hex::encode(hasher.finalize());
            println!("{}", blob_sha);

            if should_write {
                let directory = directory_from_sha(&blob_sha)?;
                fs::create_dir_all(&directory)?;

                let filename = filename_from_sha(&blob_sha)?;
                let blob_file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(filename)?;
                let mut encoder = ZlibEncoder::new(blob_file, flate2::Compression::default());
                encoder.write(&buf)?;
            }
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
        .subcommand(
            Command::new("hash-object")
                .about("Compute object ID and optionally creates a blob from a file")
                .arg(
                    Arg::new("write")
                        .short('w')
                        .action(ArgAction::SetTrue)
                        .help("Write the object into the object database"),
                ).arg(
                Arg::new("file")
                    .value_name("FILE")
                    .help("Read the object from the given file"),
            )
        )
        .get_matches()
}

fn filename_from_sha(sha: &str) -> anyhow::Result<String> {
    if sha.len() != 40 {
        return Err(anyhow!("Invalid SHA: {}", sha));
    }
    Ok(format!(".git/objects/{}/{}", &sha[..2], &sha[2..]))
}

fn directory_from_sha(sha: &str) -> anyhow::Result<String> {
    if sha.len() != 40 {
        return Err(anyhow!("Invalid SHA: {}", sha));
    }
    Ok(format!(".git/objects/{}", &sha[..2]))
}