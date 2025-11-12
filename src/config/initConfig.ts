import fs from "fs";
import { defaultConfigPath, defaultConfigYml } from "./defaultConfigYml.js";
import { DatumConfig } from "./loadConfig.js";

export function initConfig(): DatumConfig {
  const newConfig = defaultConfigYml;
  fs.mkdirSync(defaultConfigPath.replace(/\/[^/]+$/, ""), { recursive: true });
  fs.writeFileSync(defaultConfigPath, newConfig.toString(), {
    encoding: "utf-8",
  });
  return newConfig.toJSON() as DatumConfig;
}
