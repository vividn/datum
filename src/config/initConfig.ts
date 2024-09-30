import prompts, { PromptObject } from "prompts";
import yaml from "yaml";
import fs from "fs";
import { InitCmdArgs } from "../commands/initCmd";

export async function initConfig(args: InitCmdArgs): Promise<yaml.Document> {
  const newConfig = yaml.parseDocument(
    fs.readFileSync(__dirname + "/defaultConfig.yml", "utf8"),
  );

  // prompts.override({ projectDir: "abcde" });

  let isLocalCouchRunning;
  try {
    await fetch("http://localhost:5984");
    isLocalCouchRunning = true;
  } catch {
    isLocalCouchRunning = false;
  }

  const defaults = {
    projectDir: args.projectDir ?? (newConfig.get("project_dir") as string),
    dbType: isLocalCouchRunning ? "couchdb" : "pouchdb",
    host:
      args.host ??
      process.env["COUCHDB_HOST"] ??
      (isLocalCouchRunning ? "http://localhost:5984" : "%DATA%/datum"),
    user:
      args.user ??
      process.env["COUCHDB_USER"] ??
      (newConfig.getIn(["connection", "user"]) as string | null),
    password: null,
    db: args.db ?? (newConfig.get("db") as string),
  };

  const questions: PromptObject[] = [
    {
      name: "projectDir",
      type: "text",
      message:
        "Project directory. This is where you will set up and maintain your custom views, specs, hardcoded data documents, and other files. Datum then uses this directory to automatically setup each database. It is recommended to put this under version control.",
      initial: defaults.projectDir,
    },
    {
      name: "dbType",
      type: "select",
      message: "Database type",
      choices: [
        { title: "CouchDB", value: "couchdb" },
        { title: "PouchDB", value: "pouchdb" },
      ],
      initial: isLocalCouchRunning ? 0 : 1,
    },
    {
      name: "host",
      type: "text",
      message: (_, values) =>
        values.dbType === "couchdb"
          ? "CouchDB location with port"
          : "PouchDB database root location. %DATA% is the XDG_DATA_HOME directory",
      initial: (_, values) =>
        args.host ??
        (process.env["COUCHDB_HOST"] ||
          (values.dbType === "couchdb"
            ? "http://localhost:5984"
            : "%DATA%/datum")),
    },
    {
      name: "user",
      type: (_, values) => (values.dbType === "couchdb" ? "text" : null),
      message: "Default CouchDB username",
      initial: defaults.user ?? "user",
    },
    {
      name: "password",
      type: (_, values) => (values.dbType === "couchdb" ? "password" : null),
      message:
        "Couchdb password. WARNING: entering here will store in plain text in the config file. Leave blank to prompt at runtime. Can also use $COUCHDB_PASSWORD or --password",
    },
    {
      name: "db",
      type: "text",
      message: "Default database name",
      initial: "datum",
    },
  ];

  const answers = args.nonInteractive
    ? defaults
    : await prompts(questions, { onCancel: () => process.exit(1) });

  newConfig.set("project_dir", answers.projectDir);
  newConfig.set("db", answers.db);
  newConfig.setIn(["connection", "host"], answers.host);
  if (answers.user) {
    newConfig.setIn(["connection", "user"], answers.user);
  }
  if (answers.password) {
    newConfig.setIn(["connection", "password"], answers.password);
  }

  return newConfig;
}
