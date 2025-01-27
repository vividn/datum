import yaml from "yaml";
import { defaultConfigPath, defaultConfigYml } from "./defaultConfigYml";
import fs from "fs";
import { DatumConfig } from "./loadConfig";

export function initConfig(): DatumConfig {
  const newConfig = yaml.parseDocument(defaultConfigYml);
  fs.mkdirSync(defaultConfigPath.replace(/\/[^/]+$/, ""), { recursive: true });
  fs.writeFileSync(defaultConfigPath, newConfig.toString(), {
    encoding: "utf-8",
  });
  return newConfig.toJSON() as DatumConfig;
}
