NOTE: This software is still in alpha phase and is not feature complete. Expect breaking changes and instability until version 2.1.0 is released

# Datum
Datum is a personal database for collecting and analyzing data on your life

Record timestamped occur data for point-based time series data tracking:
`datum occur alcohol type=beer`
`datum occur vitamin -t 9:15am` # flexibly control time of occurence, relative and absolute

Record start/end data for block-based time series data tracking:
`datum start sleep`
`datum end sleep`


Record state based time series data:
`datum switch project emails`
`datum switch project meeting`
`datum switch project end`


Even record non time based arbitrary data:
`datum add person name=Nate city=Berlin`


## Installation
#### Latest Release
`pnpm install --global @vividn/datum`
`npm install -g @vividn/datum`
`yarn global add @vividn/datum`

#### Development
in repo: `pnpm run system`


## Setting up CouchDB
Datum uses CouchDB/PouchDB as it's database backend. If you want to use CouchDB, you'll need to set it up on your computer.

See installation instructions here: https://docs.couchdb.org/en/stable/

As admin, setup a `datum` database inside of couchdb and make sure your user has admin rights to this database.

If you want to use PouchDB locally on your file system, no steps are required.

## Initializing Datum
`datum init` - this sets up a datumrc file at `$XDG_CONFIG_HOME/datum/datumrc.yml`. It will automatically detect if you have CouchDB running on http://localhost:5984 and set up options for you


