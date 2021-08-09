import { DocumentScope } from "nano";
import { EitherDocument, EitherPayload } from "./DatumDocument";
import { UpdateStrategyNames } from "./combineData";

type updateDocType = {
  db: DocumentScope<EitherPayload>;
  id: string;
  payload: EitherPayload;
  updateStrategy?: UpdateStrategyNames;
};

const updateDoc = async ({db, id, payload, updateStrategy = "merge"}: updateDocType): EitherDocument => {

}

export default updateDoc;