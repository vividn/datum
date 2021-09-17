import { DocumentViewParams } from "nano";

function startsWith(
  str: string
): Required<
  Pick<DocumentViewParams, "start_key" | "end_key">
> {
  return { start_key: str, end_key: str + "\uffff\uffff\uffff\uffff" };
}

export default startsWith;
