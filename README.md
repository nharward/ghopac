# ghopac - GitHub Organization Pull And Clone

Utility to pull and/or clone all repos you have access to from one or more Github organizations. Will also `pull` from additional repos that you have already cloned if specified in the configuration file - these do not need to belong to any particular Github organizations.

# Building from source

1. Install [go](https://golang.org/ "Golang")
2. Install [glide](https://github.com/Masterminds/glide)
3. Fetch this code using `go get -d github.com/nharward/ghopac` or otherwise make sure it's in `$GOPATH/src/github.com/nharward/ghopac`
4. Change to `$GOPATH/src/github.com/nharward/ghopac` and run `glide install`
5. Build the binary: `go install github.com/nharward/ghopac`; you should have a new file called `$GOPATH/bin/ghopac`

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

* This program loosely honors the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html) in looking for the location of your configuration file; "loosely" here means that it works correctly to spec on \*nix platforms, but should also work on Windows using its environment variable and path naming conventions. However since I don't use Windows I currently have no way to verify this.
* You don't actually have to use Github organizations, you can simply omit `orgs` and just use `syncpoints`.
* `concurrency` if left unspecified will default to the number of CPU cores on your machine.
* `syncpoints` do not have to belong to any particular Github organization; they are just places where you expect `git pull` to work correctly
* If you have no configuration file, running the program will fail and output a sample file to get you started
