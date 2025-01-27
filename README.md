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


## CouchDB
Datum uses PouchDB as its default database backend. However, you can use `datum` to talk directly to CouchDB if you would like. Use `--host` or change the `host:` option in the config to point to a CouchDb server and then either set the $COUCHDB_USER and $COUCHDB_PASSWORD environment variables or set the values in the config.

See installation instructions here for installing a CouchDb: https://docs.couchdb.org/en/stable/

As admin, setup a `datum` database inside of CouchDb and make sure your user has admin rights to this database.


