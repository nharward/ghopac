# ghopac - GitHub Organization Pull And Clone

Utility to pull - and/or clone if missing on local disk - all repos you have access to from one or more Github organizations. Will also `pull` from additional repos that you have already cloned if specified in the configuration file - these do not need to belong to any particular Github organizations.

# Building from source

1. Install [rust](https://www.rust-lang.org/) (you may need to install [cargo](http://doc.crates.io/) separately depending on your platform/distribution)
2. Change to the top level source directory
3. Build the binary by running `cargo build --release`; your optimized binary should be in `./target/release/ghopac`
4. [Optional] Run `cargo install` to place `ghopac` in your `${HOME}/.cargo/bin` directory; add to your `PATH` if desired

# Using

1. Get a Github access token at https://github.com/settings/tokens; minimum access should be reading your orgs and repos
2. Create a config file in `${XDG_CONFIG_HOME}/ghopac/config.json`, example below:

        {
            "concurrency": 4,
            "verbose": true,
            "github_access_token": "<from step 1>",
            "orgs": [
                {
                    "org": "my_github_org",
                    "path": "/my/base/code/dir/for/my_github_org"
                }
            ],
            "syncpoints": [
                "/some/other/cloned/repo/dir",
                "/yet/another/separately/cloned/repo/dir"
            ]
        }

3. Run `ghopac`

# Configuration notes

* This program honors the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html) in looking for the location of your configuration file
* You don't have to use Github organizations, you can simply omit `orgs` and just use `syncpoints`
* `concurrency` if left unspecified will default to 4
* `syncpoints` do not have to belong to any particular Github organization; they are just local directories where you expect `git pull` to work correctly
* If you have no configuration file, running the program will fail and output a sample file to get you started

# TODO

* Add command line arguments that override the config file
* For orgs, add filtering in case having *all* repositories is overkill
* Check the issues list for more...
