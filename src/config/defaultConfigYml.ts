export const defaultConfigYml = `# Datum Configuration

# the directory where you will store you datum views, specs, and other (version controlled) files to setup your database
project_dir: "%HOME%/datum"

# default database to use 
# override with --db
db: "datum"

connection:
  # host can be 
  #   - a URL of a couchdb instance, e.g. http://localhost:5984
  #   - a root directory where to store db files with pouchdb, use %DATA% for XDG_DATA_HOME
  # Override with $COUCHDB_HOST or with --host
  host: "%DATA%/datum"

  # default user to use for couchdb instances. Has no effect on pouchdb
  # Override with $COUCHDB_USER or with --user
  user: "user"

  # password to use for couchdb instances. Has no effect on pouchdb
  # Override wtih $COUCHDB_PASSWORD or with --password
  password: null
` as const;
