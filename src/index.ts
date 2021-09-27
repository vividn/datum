#!/usr/bin/env node
import { baseYargs } from "./input/baseYargs";
import { DocExistsError } from "./documentControl/base";
import addCmd, { AddCmdArgs } from "./commands/addCmd";
import mapCmd, { MapCmdArgs } from "./commands/mapCmd";
import setupCmd, { SetupCmdArgs } from "./commands/setupCmd";
import { deleteCmd, DeleteCmdArgs } from "./commands/deleteCmd";

export async function main(cliInput: string | string[]): Promise<void> {
  const args = await baseYargs.parse(cliInput);
  switch (args._[0]) {
    case "add":
      await addCmd(args as unknown as AddCmdArgs);
      break;

    case "delete":
    case "del":
      await deleteCmd(args as unknown as DeleteCmdArgs);
      break;

    case "map":
      await mapCmd(args as unknown as MapCmdArgs);
      break;

    case "setup":
      await setupCmd(args as unknown as SetupCmdArgs);
      break;

    default:
      throw Error("command not recognized");
  }
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    if (err instanceof DocExistsError) {
      process.exitCode = 11;
    } else {
      console.error(err);
    }
  });
}
