import { EitherDocument } from "../documentControl/DatumDocument";
import { DocumentScope } from "nano";

async function getHumanIds (db: DocumentScope<EitherDocument>, _ids: string[]): Promise<(string | undefined)[]> {
  //pass
}

export default getHumanIds;