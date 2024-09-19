import { promises as fs } from "fs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { initConfig } from "../config/initConfig";

export type InitCmdArgs = {
  overwrite?: boolean;
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
  const configDir = `${xdgConfig}/datum`;

  await fs.mkdir(configDir, { recursive: true });
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

  const newConfig = await initConfig();
  await fs.writeFile(filepath, newConfig.toString(), { encoding: "utf-8" });
}
