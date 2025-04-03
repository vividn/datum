import { defaultConfigPath, defaultConfigYml } from "./defaultConfigYml";
import { DatumConfig } from "./loadConfig";

export function initConfig(): DatumConfig {
  const fs = eval("require('fs')");
  const newConfig = defaultConfigYml;
  fs.mkdirSync(defaultConfigPath.replace(/\/[^/]+$/, ""), { recursive: true });
  fs.writeFileSync(defaultConfigPath, newConfig.toString(), {
    encoding: "utf-8",
  });
  return newConfig.toJSON() as DatumConfig;
}
