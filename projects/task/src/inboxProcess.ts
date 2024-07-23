import { ArgumentParser } from "argparse";
import { dbArgs, DbArgs } from "../../../src/input/dbArgs";
import { parseIfNeeded } from "../../../src/utils/parseIfNeeded";
import { mapCmd, MapCmdArgs } from "../../../src/commands/mapCmd";
import { inboxView } from "../views/inbox";
import { Show } from "../../../src/input/outputArgs";

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
  const mapArgs: MapCmdArgs = {
    ...args,
    mapName: inboxView.name,
    params: { limit: 1, include_docs: true },
    show: Show.None
  };
  while (true) {
    const nextTask = ( await mapCmd(mapArgs) ).rows[0];
    if (nextTask === undefined) {
      break;
    }

    

  }
}
