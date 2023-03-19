// Prioritize ipv4 to work with test containers
// TODO: Maybe remove this after switching to PouchDB
import { setDefaultResultOrder } from "dns";
setDefaultResultOrder("ipv4first");

// Ensure jest always uses the test database
process.env["COUCHDB_USER"] = "admin";
process.env["COUCHDB_PASSWORD"] = "password"
process.env["COUCHDB_HOST"] = ".pouch"
process.env["POUCHDB_ADAPTER"] = "memory"