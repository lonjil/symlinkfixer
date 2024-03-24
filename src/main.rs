use cap_std::fs;

use rand::distributions::DistString;
use rustix::{fd::AsFd, path::Arg};
use tracing::Level;
#[allow(unused_imports)]
use tracing::{debug, error, info, span, trace, warn};
use tracing_subscriber::fmt;

use clap::{Parser, Subcommand};
use std::{
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::abort,
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Finds symlinks into the old directory, and repalces them with symlinks to the new directory.
    Fix {
        /// Old symlink target prefix.
        #[arg(long, value_name = "DIR")]
        old: PathBuf,
        /// New symlink target prefix.
        #[arg(long, value_name = "DIR")]
        new: PathBuf,
        /// All the directories to scan for symlinks.
        #[arg(required = true)]
        dirs: Vec<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let level = match cli.debug {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };
    let _format = fmt::fmt()
        .with_file(true)
        .with_line_number(true)
        .with_max_level(level)
        .init();

    match cli.command {
        Some(Commands::Fix { old, new, dirs }) => {
            do_fixes(old, new, dirs);
        }
        None => println!("Please select a command."),
    }
}

fn do_fixes(old: PathBuf, new: PathBuf, dirs: Vec<PathBuf>) {
    let (newd, dirsd) = {
        let auth = cap_std::ambient_authority();
        let newd = match fs::Dir::open_ambient_dir(&new, auth) {
            Ok(dir) => dir,
            Err(e) => {
                error!("Can't open new dir: {}. Aborting...", e);
                abort()
            }
        };
        let dirsd: Vec<fs::Dir> = dirs
            .iter()
            .map(|x| match fs::Dir::open_ambient_dir(x, auth) {
                Ok(dir) => dir,
                Err(e) => {
                    error!(
                        "Can't open directory to be scanned {:?}: {}. Aborting...",
                        x, e
                    );
                    abort()
                }
            })
            .collect();
        (newd, dirsd)
    };

    for (d, name) in dirsd.iter().zip(dirs) {
        let aaa = name.to_string_lossy().into_owned();
        span!(Level::DEBUG, "walkdir", dir = aaa)
            .in_scope(|| walkdir(d, perform_fix, &newd, &new, &old))
    }
}

fn walkdir(
    work_dir: &fs::Dir,
    leaf_fun: fn(
        work_dir: &fs::Dir,
        new_dir: &fs::Dir,
        new_prefix: &Path,
        old_prefix: &Path,
        file: fs::DirEntry,
    ),
    new_dir: &fs::Dir,
    new_prefix: &Path,
    old_prefix: &Path,
) {
    for x in work_dir.entries().unwrap() {
        let entry = match x {
            Ok(entry) => {
                info!("{:?}", entry);
                entry
            }
            Err(e) => {
                error!("Something wrong: {}", e);
                abort()
            }
        };
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            let aaa = entry.file_name().to_string_lossy().into_owned();
            span!(Level::DEBUG, "walkdir", dir = aaa).in_scope(|| {
                walkdir(
                    &entry.open_dir().unwrap(),
                    leaf_fun,
                    new_dir,
                    new_prefix,
                    old_prefix,
                )
            })
        } else if kind.is_symlink() {
            leaf_fun(&work_dir, new_dir, new_prefix, old_prefix, entry);
        }
    }
}

fn perform_fix(
    work_dir: &fs::Dir,
    new_dir: &fs::Dir,
    new_prefix: &Path,
    old_prefix: &Path,
    file: fs::DirEntry,
) {
    let work_dir = work_dir.open(".").unwrap();
    let work_dir_fd = work_dir.as_fd();
    let path = rustix::fs::readlinkat(work_dir_fd, file.file_name(), Vec::new()).unwrap();
    let path: &Path = OsStr::from_bytes(path.to_bytes()).as_ref();
    if path.starts_with(old_prefix) {
        info!("current path: {:?}", path);
        let suffix = path.strip_prefix(old_prefix).unwrap();
        let new_path = new_prefix.join(suffix);
        info!("    new path: {:?}", new_path);
        if new_dir.exists(suffix) {
            let rstring =
                rand::distributions::Alphanumeric {}.sample_string(&mut rand::thread_rng(), 16);
            let mut name = file.file_name();
            name.push(".");
            name.push(rstring);
            name.push(".tmp");
            debug!("tmp name: {:?}", &name);
            debug!("symlinkat({:?}, {:?}, {:?})", new_path, work_dir_fd, name);
            rustix::fs::symlinkat(&new_path, work_dir_fd, &name).unwrap();
            debug!("fsync({:?})", work_dir_fd);
            rustix::fs::fsync(work_dir_fd).unwrap();
            debug!(
                "renameat({:?}, {:?}, {:?}, {:?})",
                work_dir_fd,
                name,
                work_dir_fd,
                file.file_name()
            );
            rustix::fs::renameat(work_dir_fd, &name, work_dir_fd, file.file_name()).unwrap();
            debug!("fsync({:?})", work_dir_fd);
            rustix::fs::fsync(work_dir_fd).unwrap();
        } else {
            info!("target doesn't exist, not updating link")
        }
    } else {
        debug!("current path: {:?}, which doesn't match.", path);
    }
}
