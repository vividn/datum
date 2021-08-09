import { DocumentScope } from "nano";
import { EitherPayload } from "./DatumDocument";
import { UpdateStrategyNames } from "./combineData";

type updateDocType = {
  db: DocumentScope<EitherPayload>;
  id: string;
  payload: EitherPayload;
  updateStrategy: UpdateStrategyNames;
};

