import { DatumData } from "../documentControl/DatumDocument";
import { AddCmdArgs } from "../commands/addCmd";
import { StateArgs } from "../commands/switchCmd";
import { jClone } from "../utils/jClone";
import { BaseArgs } from "../input/baseArgs";
import { DatumState } from "../views/datumViews/activeStateView";

export async function compileState(payloadData: DatumData, args: BaseArgs): DatumData {
  const data = jClone(payloadData);
  if (args.!== undefined) {
    data.state = args.state;
  }
}