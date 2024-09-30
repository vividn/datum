// Prioritize ipv4 to work with test containers
// TODO: Maybe remove this after switching to PouchDB
import { setDefaultResultOrder } from "dns";
setDefaultResultOrder("ipv4first");

// Ensure jest always uses the test database
process.env["COUCHDB_USER"] = "admin";
process.env["COUCHDB_PASSWORD"] = "password"
process.env["COUCHDB_HOST"] = "%MEMORY%"
process.env["POUCHDB_ADAPTER"] = "memory"

// disable colored chalk unless specifically enabled
process.env["FORCE_COLOR"] = "0"

// mock load config
jest.mock("./src/config/loadConfig");
