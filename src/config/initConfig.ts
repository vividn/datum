import yaml from "yaml";
import { InitCmdArgs } from "../commands/initCmd";
import { defaultConfigYml } from "./defaultConfigYml";

export async function initConfig(args: InitCmdArgs): Promise<yaml.Document> {
  const newConfig = yaml.parseDocument(defaultConfigYml);

  return newConfig;
}
