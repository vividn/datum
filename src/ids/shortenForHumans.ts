import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";

async function shortenForHumans(
  db: DocumentScope<EitherPayload>,
  ids: string[]
): (string | undefined)[] {}

export default shortenForHumans;
