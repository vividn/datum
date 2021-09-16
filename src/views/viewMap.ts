import { DocumentScope, DocumentViewParams, DocumentViewResponse } from "nano";
import { EitherDocument, EitherPayload } from "../documentControl/DatumDocument";
import { DatumView } from "./viewDocument";

type ViewMapType = {
  db: DocumentScope<EitherPayload>;
  datumView: DatumView;
  params?: Omit<DocumentViewParams, "reduce">;
}
async function viewMap ({db, datumView, params }: ViewMapType): DocumentViewResponse<any, EitherPayload> {

}

export default viewMap;