import fs from "fs";
import yaml from "yaml";
import { MainDatumArgs } from "../input/mainArgs";

export type DatumConfig = {
  db?: string;
  host?: string;
  user?: string;
  password?: string;
};

export function loadConfig(args: MainDatumArgs): DatumConfig {
  const configDir =
    process.env["XDG_CONFIG_HOME"] || `${process.env["HOME"]}/.config`;
  const configFile = args.configFile ?? `${configDir}/datum/datumrc.yml`;
  let config: DatumConfig;
  try {
    config = yaml
      .parseDocument(fs.readFileSync(configFile, "utf8"))
      .toJSON() as DatumConfig;
  } catch (e: any) {
    if (e.code === "ENOENT") {
      if (args.configFile) {
        throw new Error(`Config file not found: ${args.configFile}`);
      } else {
        console.info(`Welcome to datum!`);
        console.info(`Creating a configuration file at ${configFile}`);
        
      }
    } else {
      throw e;
    }
  }

  if (config.project_dir) {
    config.project_dir = config.project_dir.replaceAll(
      /%HOME%|~/g,
      process.env["HOME"]!,
    );
    config.project_dir = config.project_dir.replaceAll(
      "%DATA%",
      process.env["XDG_DATA_HOME"] ?? process.env["HOME"] + "/.local/share",
    );
  }

  if (config.connection?.host) {
    config.connection.host = config.connection.host.replaceAll(
      /%HOME%|~/g,
      process.env["HOME"]!,
    );
    config.connection.host = config.connection.host.replaceAll(
      "%DATA%",
      process.env["XDG_DATA_HOME"] ?? process.env["HOME"] + "/.local/share",
    );
  }

  return config;
}
