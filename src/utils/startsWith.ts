import { DocumentListParams, DocumentViewParams } from "nano";

function startsWith(str: string): Required<Pick<DocumentViewParams, "start_key" | "end_key" | "inclusive_end">> {
  const lastChar = str.slice(-1);
  const lastCharPlusOne = String.fromCharCode(lastChar.charCodeAt(0) + 1);
  const strPlusOne = str.slice(0,-1) + lastCharPlusOne;

  return {start_key: str, end_key: strPlusOne, inclusive_end: false};
}

export default startsWith;