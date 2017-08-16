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

#[derive(Debug, Serialize, Deserialize)]
struct ConfigOrg {
    org: String,
    path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    github_access_token: Option<String>,
    orgs:                Option<Vec<ConfigOrg>>,
    syncpoints:          Option<Vec<String>>,
    concurrency:         Option<u8>,
    verbose:             Option<bool>,
}

#[derive(Debug)]
struct GitRepoSyncRequest {
    path: path::PathBuf,
    clone_url: Option<String>,
}

const PROGRAM_NAME: &'static str = "ghopac";
const CONFIG_FILE:  &'static str = "config.json";
const DEFAULT_CONCURRENCY: u8 = 4;

fn show_config_sample_and_exit_1() -> path::PathBuf {
    let sample_config = Config {
        github_access_token: Some("Replace with a token from https://github.com/settings/tokens".to_owned()),
        orgs: Some(vec![
            ConfigOrg {
                org: "myorg".to_owned(),
                path: "/myorg/source/directory".to_owned(),
            },
        ]),
        syncpoints: Some(vec!["/some/other/directory".to_owned()]),
        concurrency: Some(DEFAULT_CONCURRENCY),
        verbose: Some(true),
    };
    let json = serde_json::to_string_pretty(&sample_config).expect("Unable to generate a sample configuration");
    eprintln!("No config file! Here's a sample you can put into $XDG_CONFIG_HOME/{}/{}:\n\n{}\n", PROGRAM_NAME, CONFIG_FILE, json);
    process::exit(1);
}

fn configuration() -> Result<Config, Box<error::Error>> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(PROGRAM_NAME).expect("Something is wrong with your XDG environment variables");
    let config_file_path: path::PathBuf = xdg_dirs.find_config_file(path::Path::new(CONFIG_FILE)).unwrap_or_else(show_config_sample_and_exit_1);
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

fn worker_thread(config: Arc<Config>, receiver: spmc::Receiver<GitRepoSyncRequest>) -> u16 {
    let mut error_count = 0;
    loop {
        match receiver.recv() {
            Ok(request) => {
                let mut git_args = Vec::with_capacity(5);
                if request.path.exists() {
                    if request.path.is_dir() {
                        git_args.append(&mut vec!["pull", "--prune"]);
                    } else {
                        eprintln!("[FAILED]\t{} exists but is not a directory", request.path.to_str().unwrap());
                        error_count += 1;
                        continue;
                    }
                } else {
                    match request.clone_url {
                        Some(ref clone_url) => {
                            git_args.append(&mut vec!["clone", clone_url, request.path.to_str().unwrap()]);
                        },
                        None => {
                            eprintln!("[FAILED]\t{} doesn't exist and no clone URL defined", request.path.to_str().unwrap());
                            error_count += 1;
                            continue;
                        }
                    }
                }
                match process::Command::new("git")
                            .args(git_args)
                            .stdin(process::Stdio::null())
                            .current_dir(closest_ancestor_dir(Some(request.path.as_path())).unwrap())
                            .output() {
                    Ok(output) => {
                        if output.status.success() {
                            if let Some(true) = config.verbose {
                                match request.clone_url {
                                    Some(clone_url) => println!("[OK]\t{} - {}", clone_url, request.path.to_str().unwrap()),
                                    None            => println!("[OK]\t{}", request.path.to_str().unwrap()),
                                }
                            }
                        } else {
                            error_count += 1;
                            match output.status.code() {
                                Some(code) => {
                                    eprintln!("[FAILED]\tgit command for {} failed with status {}:\n----> stdout [{}]\n----> stderr [{}]",
                                              request.path.to_str().unwrap(), code,
                                              String::from_utf8_lossy(&output.stdout),
                                              String::from_utf8_lossy(&output.stderr));
                                },
                                None => {
                                    eprintln!("[FAILED]\tgit command for {} was killed external with a signal", request.path.to_str().unwrap());
                                }
                            }
                            continue;
                        }
                    },
                    Err(_) => {
                        eprintln!("[FAILED]\tunable to get exit status of git command for {}", request.path.to_str().unwrap());
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

fn main() {
    let config = Arc::new(configuration().expect("Unable to read or parse your configuration file"));
    let concurrency = config.concurrency.map_or(DEFAULT_CONCURRENCY, |v| {
        if v > 0 {
            v
        } else {
            DEFAULT_CONCURRENCY
        }
    });

    let (tx, rx) = spmc::channel();
    let mut threads = Vec::with_capacity(concurrency as usize);
    for _ in 0..concurrency {
        let config = config.clone();
        let rx = rx.clone();
        threads.push(thread::spawn(move || worker_thread(config, rx)));
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
                                eprintln!("[WARNING] Problem accessing org `{}` repository list: {}", org.org, e);
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

    let mut error_count = 0u16;
    for t in threads {
        error_count += t.join().expect("Unable to get child thread result");
    }
    process::exit(cmp::min(error_count, 255) as i32);
}
