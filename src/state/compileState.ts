import { DatumData } from "../documentControl/DatumDocument";
import { jClone } from "../utils/jClone";
import { BaseArgs } from "../input/baseArgs";

export async function compileState(
  payloadData: DatumData,
  args: BaseArgs,
): DatumData {
  const _data = jClone(payloadData);
}
