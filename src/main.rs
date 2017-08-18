// ghopac - GitHub Organization Pull And Clone
// Copyright (C) 2017 Nathaniel Harward
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! # What
//!
//! `ghopac` is _GitHub Organization Pull And Clone_ - a tool to pull/clone lots of Git
//! repositories at once with a single command. It primarily has support for
//! [Github](https://github.com/) organizations and cloning/pulling all organization repos you
//! have access to. Additionally it also supports keeping existing cloned repos up to date,
//! regardless of their origin. You can read more about it at ... wait for it ... the [ghopac
//! Github repo](https://github.com/nharward/ghopac/).
//!
//! # Why
//!
//! I found myself working at companies with a lot of repositories in their Github organizations,
//! and it was a pain to even find much less manually clone all of the ones I cared about. Disk is
//! cheap, so I wrote a [Go](https://golang.org/) program to grab them all at once  in parallel.
//! (see the [`go-legacy`](https://github.com/nharward/ghopac/tree/go-legacy) tag to see the code).
//! I re-wrote it in [Rust](https://www.rust-lang.org/) because I needed an excuse to learn it.
//!

#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

#[macro_use]
extern crate serde_derive;

extern crate hubcaps;
extern crate hyper;
extern crate hyper_native_tls;
extern crate serde_json;

extern crate spmc;
extern crate xdg;

use std::boxed::Box;
use std::cmp;
use std::error;
use std::fs;
use std::mem;
use std::path;
use std::process;
use std::sync::Arc;
use std::thread;

use hubcaps::{Credentials, Github};
use hyper::Client;
use hyper_native_tls::NativeTlsClient;
use hyper::net::HttpsConnector;

use slog::Drain;

#[derive(Serialize, Deserialize)]
struct ConfigOrg {
    org: String,
    path: String,
}

#[derive(Serialize, Deserialize)]
struct Config {
    github_access_token: Option<String>,
    orgs:                Option<Vec<ConfigOrg>>,
    syncpoints:          Option<Vec<String>>,
    concurrency:         Option<u8>,
    verbose:             Option<bool>,
}

struct GitRepoSyncRequest {
    path: path::PathBuf,
    clone_url: Option<String>,
}

const PROGRAM_NAME: &'static str = "ghopac";
const CONFIG_FILE:  &'static str = "config.json";
const DEFAULT_CONCURRENCY: u8 = 4;

fn show_config_sample_and_exit_1() -> path::PathBuf {
    let sample_config = Config {
        github_access_token: Some("Use a token from https://github.com/settings/tokens".to_owned()),
        orgs: Some(vec![
            ConfigOrg {
                org: "my_org".to_owned(),
                path: "/my_org/source/directory".to_owned(),
            },
            ConfigOrg {
                org: "some_other_org".to_owned(),
                path: "/some_other_org/source/directory".to_owned(),
            },
        ]),
        syncpoints: Some(vec!["/some/previously/cloned/directory".to_owned(), "/some/other/previously/cloned/directory".to_owned()]),
        concurrency: Some(DEFAULT_CONCURRENCY),
        verbose: Some(true),
    };
    let json = serde_json::to_string_pretty(&sample_config).expect("Unable to generate a sample configuration");
    eprintln!("No config file! Here's a sample you can put into $XDG_CONFIG_HOME/{}/{}:\n\n{}\n", PROGRAM_NAME, CONFIG_FILE, json);
    process::exit(1);
}

fn configuration(logger: slog::Logger) -> Result<Config, Box<error::Error>> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(PROGRAM_NAME).expect("Issue processing your XDG environment variables");
    let config_file_path: path::PathBuf = xdg_dirs.find_config_file(path::Path::new(CONFIG_FILE)).unwrap_or_else(show_config_sample_and_exit_1);
    debug!(logger, "Using configuration file: {:?}", config_file_path);
    let config_file = fs::File::open(config_file_path)?;
    Ok(serde_json::from_reader(config_file)?)
}

fn closest_ancestor_dir(path: Option<&path::Path>) -> Option<&path::Path> {
    match path {
        Some(path) => {
            if path.exists() && path.is_dir() {
                Some(path)
            } else {
                closest_ancestor_dir(path.parent())
            }
        },
        None => None,
    }
}

fn worker_thread(logger: slog::Logger, config: Arc<Config>, receiver: spmc::Receiver<GitRepoSyncRequest>) -> u16 {
    let mut error_count = 0;
    loop {
        match receiver.recv() {
            Ok(request) => {
                let mut git_args = Vec::with_capacity(3);
                if request.path.exists() {
                    if request.path.is_dir() {
                        git_args.append(&mut vec!["pull", "--prune"]);
                    } else {
                        error!(logger, "{} exists but is not a directory, skipping", request.path.to_str().unwrap());
                        error_count += 1;
                        continue;
                    }
                } else {
                    match request.clone_url {
                        Some(ref clone_url) => {
                            git_args.append(&mut vec!["clone", clone_url, request.path.to_str().unwrap()]);
                        },
                        None => {
                            error!(logger, "{} doesn't exist and no clone URL defined", request.path.to_str().unwrap());
                            error_count += 1;
                            continue;
                        }
                    }
                }
                debug!(logger, "Running `git {:?}` for {:?}", git_args, request.path);
                match process::Command::new("git")
                            .args(git_args)
                            .stdin(process::Stdio::null())
                            .current_dir(closest_ancestor_dir(Some(request.path.as_path())).unwrap())
                            .output() {
                    Ok(output) => {
                        if output.status.success() {
                            if let Some(true) = config.verbose {
                                match request.clone_url {
                                    Some(clone_url) => info!(logger, "Ok {} -> {}", clone_url, request.path.to_str().unwrap()),
                                    None            => info!(logger, "Ok {}", request.path.to_str().unwrap()),
                                }
                            }
                        } else {
                            error_count += 1;
                            match output.status.code() {
                                Some(code) => {
                                    error!(logger, "git command for {} failed with status {}:\n----> stdout [{}]\n----> stderr [{}]",
                                           request.path.to_str().unwrap(), code,
                                           String::from_utf8_lossy(&output.stdout),
                                           String::from_utf8_lossy(&output.stderr));
                                },
                                None => {
                                    error!(logger, "git command for {} was killed externally by a signal", request.path.to_str().unwrap());
                                }
                            }
                            continue;
                        }
                    },
                    Err(_) => {
                        error!(logger, "Unable to get exit status of git command for {}", request.path.to_str().unwrap());
                        error_count += 1;
                        continue;
                    },
                }
            },
            Err(_) => break,
        }
    }
    error_count
}

fn create_root_logger() -> slog::Logger {
    let stdout_decorator = slog_term::TermDecorator::new().stdout().build();
    let stdout_drain = slog_term::FullFormat::new(stdout_decorator).build().filter_level(slog::Level::Debug).fuse();

    let stderr_decorator = slog_term::TermDecorator::new().stderr().build();
    let stderr_drain = slog_term::FullFormat::new(stderr_decorator).build().filter_level(slog::Level::Error).fuse();

    let tee_drain = slog::Duplicate::new(stdout_drain, stderr_drain).fuse();

    let async_drain = slog_async::Async::new(tee_drain).build().fuse();

    slog::Logger::root(async_drain, o!())
}

fn main() {
    let logger = create_root_logger();
    let config = Arc::new(configuration(logger.clone()).expect("Unable to read or parse your configuration file"));
    let concurrency = config.concurrency.map_or(DEFAULT_CONCURRENCY, |v| {
        if v > 0 {
            v
        } else {
            DEFAULT_CONCURRENCY
        }
    });
    debug!(logger, "Using concurrency of {}", concurrency);

    let (tx, rx) = spmc::channel();
    let mut threads = Vec::with_capacity(concurrency as usize);
    for _ in 0..concurrency {
        let config = config.clone();
        let rx = rx.clone();
        let worker_logger = logger.clone();
        threads.push(thread::spawn(move || worker_thread(worker_logger, config, rx)));
    }

    match config.github_access_token {
        Some(ref github_token) => {
            let github = Github::new(PROGRAM_NAME, Client::with_connector(HttpsConnector::new(NativeTlsClient::new().unwrap())), Credentials::Token(github_token.to_owned()));
            let list_options = Default::default();
            match config.orgs {
                Some(ref orgs) => {
                    for org in orgs {
                        match github.org(org.org.clone()).repos().iter(&list_options) {
                            Ok(org_repos) => {
                                for org_repo in org_repos {
                                    let clone_url = if ! org_repo.ssh_url.trim().is_empty() {
                                        Some(org_repo.ssh_url)
                                    } else {
                                        None
                                    };
                                    tx.send(GitRepoSyncRequest { path: path::PathBuf::from(format!("{}{}{}", org.path.clone(), path::MAIN_SEPARATOR, org_repo.name)),
                                                                 clone_url: clone_url })
                                        .expect(format!("Unable to queue repo[{}] for org[{}]", org_repo.name, org.org).as_str());
                                }
                            },
                            Err(e) => {
                                warn!(logger, "Problem accessing org `{}` repository list, skipping: {}", org.org, e);
                            }
                        }
                    }
                },
                _ => ()
            }
        },
        _ => ()
    }

    match config.syncpoints {
        Some(ref syncpoints) => {
            for syncpoint in syncpoints {
                tx.send(GitRepoSyncRequest { path: path::PathBuf::from(syncpoint), clone_url: None })
                    .expect("Unable to queue work");
            }
        },
        _ => ()
    }
    mem::drop(tx);
    debug!(logger, "All requests queued, waiting for workers to finish");

    let mut error_count = 0u16;
    for t in threads {
        error_count += t.join().expect("Unable to get child thread result");
    }
    match cmp::min(error_count, 255) {
        0 => {},
        code => {
            mem::drop(logger);
            process::exit(code as i32);
        }
    }
}
