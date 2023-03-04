// Ensure jest always uses the test database
process.env["COUCHDB_USER"] = "admin";
process.env["COUCHDB_PASSWORD"] = "password"
process.env["COUCHDB_HOSTNAME"] = "localhost:5983"