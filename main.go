// vim: noexpandtab
package main

import (
	"encoding/json"
	"github.com/google/go-github/github"
	"golang.org/x/oauth2"
	"io/ioutil"
	"log"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"runtime"
	"strings"
)

type ConfigOrg struct {
	Org  string `json:"org"`
	Path string `json:"path"`
}

type Config struct {
	GithubAccessToken string      `json:"github_access_token"`
	Orgs              []ConfigOrg `json:"orgs,omitempty"`
	ExtraPaths        []string    `json:"syncpoints,omitempty"`
	Concurrency       int         `json:"concurrency,omitempty"`
	Verbose           bool        `json:"verbose,omitempty"`
}

type SyncSource struct {
	Path     string
	CloneURL *string
}

func exists(pathname string) bool {
	_, err := os.Stat(pathname)
	return err == nil
}

func isEmpty(s string) bool {
	return len(s) == 0 || len(strings.TrimSpace(s)) == 0
}

func configLocation() string {
	// Honor the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
	xdgConfigPath := func(configBase string) string {
		return filepath.Join(configBase, "ghopac", "config.json")
	}

	xdgDefaultConfigDir := filepath.Join(string(filepath.Separator), "etc", "xdg")
	xdgConfigDirs, xdgConfigDirsIsSet := os.LookupEnv("XDG_CONFIG_DIRS")
	xdgConfigHome, xdgConfigHomeIsSet := os.LookupEnv("XDG_CONFIG_HOME")

	if !xdgConfigHomeIsSet || isEmpty(xdgConfigHome) {
		if user, err := user.Current(); err == nil {
			xdgConfigHome = filepath.Join(user.HomeDir, ".config")
		} else {
			log.Fatalf("Unable to determine current user, please set XDG_CONFIG_HOME explicitly. Error: %v\n", err)
		}
	}

	if exists(xdgConfigPath(xdgConfigHome)) {
		return xdgConfigPath(xdgConfigHome)
	}

	if xdgConfigDirsIsSet && !isEmpty(xdgConfigDirs) {
		for _, xdgConfigDir := range strings.Split(xdgConfigDirs, string(filepath.ListSeparator)) {
			if filepath.IsAbs(xdgConfigDir) && exists(xdgConfigPath(xdgConfigDir)) {
				return xdgConfigPath(xdgConfigDir)
			}
		}
	} else if exists(xdgConfigPath(xdgDefaultConfigDir)) {
		return xdgConfigPath(xdgDefaultConfigDir)
	}

	// Doesn't exist anywhere, return where it should be
	return xdgConfigPath(xdgConfigHome)
}

func config() (conf *Config) {
	configFileLocation := configLocation()
	if exists(configFileLocation) {
		if configuration, err := ioutil.ReadFile(configFileLocation); err == nil {
			if json.Unmarshal(configuration, &conf) != nil {
				log.Fatalf("Can't parse your config file[%v]. Try removing it and running again.", configFileLocation)
			}
		} else {
			log.Fatalf("Unable to read your config file[%v]: %v", configFileLocation, err)
		}
	}
	if conf == nil {
		log.Printf("No config file! Here's a sample you can put into %v:\n\n", configFileLocation)
		sampleConfig := &Config{
			GithubAccessToken: "Replace with a token from https://github.com/settings/tokens",
			Orgs:              []ConfigOrg{ConfigOrg{Org: "myorgname", Path: filepath.Join(string(filepath.Separator), "some", "source", "directory")}},
			ExtraPaths:        []string{filepath.Join(string(filepath.Separator), "some", "other", "directory")},
			Concurrency:       runtime.NumCPU(),
			Verbose:           true,
		}
		if marshalledConfig, err := json.MarshalIndent(sampleConfig, "", "    "); err == nil {
			os.Stderr.Write(marshalledConfig)
			os.Stderr.WriteString("\n")
		} else {
			log.Fatalf("Unable to generate sample config file! Someone broke this program. Go find them.\n")
		}
	}
	return
}

func syncRepositoryWorker(sources chan SyncSource, done chan bool, verbose bool) {
	allGood := true
	for source := range sources {
		var command *exec.Cmd
		if exists(source.Path) {
			command = exec.Command("git", "pull", "--prune")
			command.Dir = source.Path
		} else if source.CloneURL != nil {
			command = exec.Command("git", "clone", *source.CloneURL, source.Path)
		} else {
			log.Println("[WARN] Unable to sync directory %v as it does not exist, skipping.\n", source.Path)
		}
		if err := command.Run(); err != nil {
			if source.CloneURL != nil {
				log.Printf("[FAILED]\t%v - %v -> %v\n", *source.CloneURL, source.Path, err)
			} else {
				log.Printf("[FAILED]\t%v -> %v\n", source.Path, err)
			}
			allGood = false
		} else {
			if verbose {
				if source.CloneURL != nil {
					log.Printf("[OK]\t%v - %v\n", *source.CloneURL, source.Path)
				} else {
					log.Printf("[OK]\t%v\n", source.Path)
				}
			}
			allGood = allGood && command.ProcessState.Success()
		}
	}
	done <- allGood
}

func main() {
	configP := config()
	if configP == nil {
		log.Fatalf("No config file specified.")
		os.Exit(1)
	}
	config := *configP

	var concurrency = runtime.NumCPU()
	if config.Concurrency > 0 {
		concurrency = config.Concurrency
	}

	drains := make([]chan bool, concurrency)
	synclist := make(chan SyncSource, 1000)
	for i := 0; i < concurrency; i++ {
		drains[i] = make(chan bool, 1)
		go syncRepositoryWorker(synclist, drains[i], config.Verbose)
	}

	allGood := true
	for _, org := range config.Orgs {
		if exists(org.Path) {
			tokenSource := oauth2.StaticTokenSource(&oauth2.Token{AccessToken: config.GithubAccessToken})
			client := github.NewClient(oauth2.NewClient(oauth2.NoContext, tokenSource))
			options := &github.RepositoryListByOrgOptions{
				ListOptions: github.ListOptions{PerPage: 25},
			}
			for {
				// Page through the list of repositories
				repos, response, err := client.Repositories.ListByOrg(org.Org, options)
				if err != nil {
					log.Printf("[WARNING] Problem accessing org `%v` repository list page %v: %v\n", org.Org, options.ListOptions.Page, err)
					allGood = false
					break
				}
				for _, repo := range repos {
					synclist <- SyncSource{
						Path:     filepath.Join(org.Path, *repo.Name),
						CloneURL: repo.SSHURL,
					}
				}
				if response.NextPage == 0 {
					break
				}
				options.ListOptions.Page = response.NextPage
			}
		} else {
			allGood = false
			log.Printf("[WARNING] Source directory %v for org %v does not exist, skipping.\n", org.Path, org.Org)
		}
	}
	for _, extraPath := range config.ExtraPaths {
		if exists(extraPath) {
			synclist <- SyncSource{Path: extraPath}
		} else {
			log.Printf("[WARNING] Source directory %v does not exist, skipping.\n", extraPath)
			allGood = false
		}
	}
	close(synclist)

	for _, drain := range drains {
		allGood = allGood && <-drain
	}
	if allGood {
		os.Exit(0)
	} else {
		os.Exit(1)
	}
}
