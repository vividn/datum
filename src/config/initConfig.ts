import yaml from "yaml";
import { defaultConfigPath, defaultConfigYml } from "./defaultConfigYml";
import fs from "fs/promises";
import { DatumConfig } from "./loadConfig";

export async function initConfig(): Promise<DatumConfig> {
  const newConfig = yaml.parseDocument(defaultConfigYml);
  await fs.writeFile(defaultConfigPath, newConfig.toString(), {
    encoding: "utf-8",
  });
  return newConfig.toJSON() as DatumConfig;
}
