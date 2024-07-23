import readline from "node:readline/promises";
import { stdin, stdout } from "node:process";
import { ArgumentParser } from "argparse";
import { dbArgs, DbArgs } from "../../../src/input/dbArgs";
import { parseIfNeeded } from "../../../src/utils/parseIfNeeded";
import { mapCmd, MapCmdArgs } from "../../../src/commands/mapCmd";
import { inboxView } from "../views/inbox";
import { Show } from "../../../src/input/outputArgs";
import { getCmd } from "../../../src/commands/getCmd";
import { updateCmd } from "../../../src/commands/updateCmd";
import { deleteCmd } from "../../../src/commands/deleteCmd";

type InboxProcessArgs = DbArgs;

const inboxProcessArgs = new ArgumentParser({
  description: "Process task inbox one by one",
  add_help: true,
  prog: "inboxProcess",
  parents: [dbArgs],
});

inboxProcessArgs.set_defaults({
  db: "task",
});

export async function inboxProcess(
  cliOrArgs: InboxProcessArgs | string | string[],
): Promise<void> {
  const args = parseIfNeeded(inboxProcessArgs, cliOrArgs);
  const rl = readline.createInterface({
    input: stdin,
    output: stdout,
    terminal: true,
  });

  const mapArgs: MapCmdArgs = {
    ...args,
    mapName: inboxView.name,
    params: { limit: 1, include_docs: true },
    show: Show.None,
  };
  while (true) {
    const nextTask = (await mapCmd(mapArgs)).rows[0];
    if (nextTask === undefined) {
      break;
    }

    console.log(
      "`done`, `del`, or `[project] [estimatedDur] [extraKey=extraValue...]. Empty line advances to next task once project is assigned.`",
    );
    getCmd(nextTask.id);
    while (true) {
      console.log("--------------------");
      const input = await rl.question(">");
      if (input === "") {
        break;
      } else if (input === "done") {
        updateCmd(`${nextTask.id} done=true`);
      } else if (input === "del" || input === "delete") {
        deleteCmd(nextTask.id);
      } else {
        updateCmd(`${nextTask.id} -k project -k estimatedDur ${input}`)
      }
    }
  }
}
if (require.main === module) {
  inboxProcess(process.argv.slice(2)).catch((err) => {
    throw err;
  });
}
