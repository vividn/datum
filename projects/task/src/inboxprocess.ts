import { parse as shellParse } from "shell-quote";
import readline from "node:readline/promises";
import { stdin, stdout } from "node:process";
import { ArgumentParser } from "argparse";
import { dbArgs, DbArgs } from "../../../src/input/dbArgs";
import { parseIfNeeded } from "../../../src/utils/parseIfNeeded";
import { mapCmd, MapCmdArgs } from "../../../src/commands/mapCmd";
import { inboxView, TaskDoc } from "../views/inbox";
import { outputArgs, OutputArgs, Show } from "../../../src/input/outputArgs";
import { getCmd } from "../../../src/commands/getCmd";
import { updateCmd } from "../../../src/commands/updateCmd";
import { deleteCmd } from "../../../src/commands/deleteCmd";

type InboxProcessArgs = DbArgs & OutputArgs;

const inboxProcessArgs = new ArgumentParser({
  description: "Process task inbox one by one",
  add_help: true,
  prog: "inboxProcess",
  parents: [dbArgs, outputArgs],
});

inboxProcessArgs.set_defaults({
  db: "task",
});

export async function inboxProcess(
  cliOrArgs: InboxProcessArgs | string | string[],
): Promise<void> {
  const args = parseIfNeeded(inboxProcessArgs, cliOrArgs);
  args.show ??= Show.Default;
  const rl = readline.createInterface({
    input: stdin,
    output: stdout,
    terminal: true,
  });

  const mapArgs: MapCmdArgs = {
    ...args,
    mapName: inboxView.name,
    params: { limit: 1},
    show: Show.None,
  };
  while (true) {
    const nextTask = (await mapCmd(mapArgs)).rows[0];
    if (nextTask === undefined) {
      break;
    }

    console.clear();
    console.log(
      "`done`, `del`, or `[project] [estimatedDur] [extraKey=extraValue...]. Empty line advances to next task once project is assigned. Without a project, an empty line changes type=pending`",
    );
    let task = (await getCmd([nextTask.id], args))[0] as TaskDoc;
    while (true) {

      console.log("--------------------");
      const input = await rl.question(">");
      if (input === "" && task.data.type !== "inbox") {
        break;
      } else if (input === "done") {
        task = (await updateCmd([task._id, "done=true"], args))[0] as TaskDoc;
      } else if (input === "del" || input === "delete") {
        await deleteCmd([task._id], args);
      } else {
        task = (await updateCmd(
          [
            task._id,
            "type=pending",
            "-k",
            "proj=",
            "-k",
            "estimatedDur=",
            ...(shellParse(input) as string[]),
          ],
          args,
        ))[0] as TaskDoc;
      }
    }
  }
}
if (require.main === module) {
  inboxProcess(process.argv.slice(2)).catch((err) => {
    throw err;
  });
}
