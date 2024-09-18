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

  const questions: PromptObject[] = [
    {
      name: "project_dir",
      type: "text",
      message: "Project directory",
      hint: "this is a hint",
    },
  ];

  const answers = await prompts(questions, { onCancel: () => process.exit(1) });
  console.log({ answers });
}
