import { promises as fs } from "fs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import prompts, { PromptObject } from "prompts";

export type InitCmdArgs = {
  overwrite?: boolean;
  global?: boolean;
  nonInteractive?: boolean;
};

export const initCmdArgs = new ArgumentParser({
  description: "initialize the datum configuration",
  prog: "datum init",
  usage: `%(prog)s [--overwrite] [--global]`,
});

initCmdArgs.add_argument("--overwrite", {
  help: "overwrite the existing configuration file",
  action: "store_true",
});
initCmdArgs.add_argument("--global", {
  help: "initialize the global configuration file",
  action: "store_true",
});
initCmdArgs.add_argument("--non-interactive", {
  help: "do not prompt for input",
  action: "store_true",
});

export async function initCmd(
  args: InitCmdArgs | string | string[],
  preparsed?: Partial<InitCmdArgs>,
): Promise<void> {
  args = parseIfNeeded(initCmdArgs, args, preparsed);

  const xdgConfig =
    process.env.XDG_CONFIG_HOME ?? `${process.env.HOME}/.config`;
  const configDir = args.global ? xdgConfig : process.cwd();

  if (!args.global) {
    throw new Error("only --global initialization is supported at the moment");
  } else {
    await fs.mkdir(configDir, { recursive: true });
  }

  const filepath = `${configDir}/datumrc.yml`;
  let fileExists;
  try {
    await fs.access(filepath);
    fileExists = true;
  } catch {
    fileExists = false;
  }

  if (fileExists && !args.overwrite) {
    throw new Error(
      `datumrc.yml already exists at ${filepath}. Use --overwrite to overwrite.`,
    );
  }

  let isLocalCouchRunning;
  try {
    await fetch("http://localhost:5984");
    isLocalCouchRunning = true;
  } catch {
    isLocalCouchRunning = false;
  }

  const questions: PromptObject[] = [
    {
      name: "projectDir",
      type: "text",
      message:
        "Project directory. This is where you will set up and maintain your custom views, specs, hardcoded data documents, and other files. Datum then uses this directory to automatically setup each database. It is recommended to put this under version control.",
      initial: "~/datum",
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
        values.dbType === "couchdb" ? "http://localhost:5984" : "%DATA%/datum",
    },
    {
      name: "user",
      type: (_, values) => (values.dbType === "couchdb" ? "text" : null),
      message: "Default CouchDB username",
      initial: "user",
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

  const answers = await prompts(questions, { onCancel: () => process.exit(1) });
  console.log({ answers });
}
