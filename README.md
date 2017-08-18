# What

`ghopac` is _GitHub Organization Pull And Clone_ - a tool to pull/clone lots of Git repositories in parallel with a single command. Supports cloning/pulling all [Github](https://github.com/) organization repositories you have access to, as well as keeping any other cloned repositories up to date regardless of their origin. Either or both features can be used.

# Why

I found myself working at companies with a lot of repositories in their Github organizations, and it was a pain to even find much less manually clone all of the ones I cared about. Disk is cheap, so I wrote a [Go](https://golang.org/) program to grab them all at once in parallel and put them into a single top level directory. The [`go-legacy`](https://github.com/nharward/ghopac/tree/go-legacy) tag has the original code. I re-wrote it in [Rust](https://www.rust-lang.org/) because I needed an excuse to learn it.

# Installation

1. Install [rust](https://www.rust-lang.org/) (you may need to install [cargo](http://doc.crates.io/) separately depending on your platform/distribution)
2. Run `cargo install ghopac` to place the `ghopac` binary in your `${HOME}/.cargo/bin` directory; add to your `PATH` if desired

# Using

1. Create a Github access token at https://github.com/settings/tokens; minimum access should be reading your orgs and repos
2. Create a config file in `${XDG_CONFIG_HOME}/ghopac/config.json`, example below:
   ```json
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
   ```
3. Run `ghopac`

# Configuration notes

* This program honors the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html) in looking for the location of your configuration file
* You don't have to use Github organizations, you can simply omit `orgs` and just use `syncpoints` (or vice versa)
* `concurrency` defaults to `4` if left unspecified in the configuration file
* `syncpoints` do not have to belong to any particular Github organization; they are just local directories where you expect `git pull` to work correctly
* If you have no configuration file, running the program will fail and output a sample file to get you started

# TODO

* Add command line arguments that override the config file
* For orgs, add filtering in case having *all* repositories is overkill
* Check the issues list for more...
