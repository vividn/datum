import yaml from "yaml";

export const defaultConfigDir =
  process.env["XDG_CONFIG_HOME"] || `${process.env["HOME"]}/.config`;
export const defaultConfigPath = `${defaultConfigDir}/datum/datumrc.yml`;

export const defaultConfigYml = yaml.parseDocument(`# Datum Configuration

# default database to use 
# override with --db
db: "datum"

# host can be 
#   - a root directory where to store db files with pouchdb, use %DATA% for XDG_DATA_HOME
#   - a URL of a couchdb instance, e.g. http://localhost:5984
# Override with $DATUM_HOST or with --host
host: "%DATA%/datum"

# For couchdb connnections you can specify a user and password here, override with $COUCHDB_USER, $COUCHDB_PASSWORD or --user, --password
# Has no effect for pouchdb connections
# Leave blank or null to prompt
# user: null
# password: null
`);
