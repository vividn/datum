#!/usr/bin/env node
import { baseYargs } from "./input/baseYargs";
import { DocExistsError } from "./documentControl/base";
import addCmd, { AddCmdArgs } from "./commands/addCmd";

async function main(cliInput: string | string[]): Promise<void> {
  const args = await baseYargs.parseAsync(cliInput);
  switch (args._[0]) {
    case "add":
      await addCmd(args as AddCmdArgs);
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
