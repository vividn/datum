import yaml from "yaml";
import { MainDatumArgs } from "../input/mainArgs";
import { initConfig } from "./initConfig";
import { defaultConfigPath } from "./defaultConfigYml";

export type DatumConfig = {
  db?: string;
  host?: string;
  user?: string;
  password?: string;
};

export function loadConfig(args: MainDatumArgs): DatumConfig {
  const fs = eval("require('fs')");
  const configFile = args.configFile ?? defaultConfigPath;
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
        console.info("");
        config = initConfig();
      }
    } else {
      throw e;
    }
  }

  if (config?.host) {
    config.host = config.host.replaceAll(/%HOME%|~/g, process.env["HOME"]!);
    config.host = config.host.replaceAll(
      "%DATA%",
      process.env["XDG_DATA_HOME"] ?? process.env["HOME"] + "/.local/share",
    );
  }

  return config;
}
