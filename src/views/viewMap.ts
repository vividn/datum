import { DocumentScope, DocumentViewParams, DocumentViewResponse } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { DatumView } from "./viewDocument";
import { DatumViewMissingError, isCouchDbError } from "../errors";

type ViewMapType = {
  db: DocumentScope<EitherPayload>;
  datumView: DatumView;
  params?: Omit<DocumentViewParams, "reduce">;
};
async function viewMap({
  db,
  datumView,
  params,
}: ViewMapType): Promise<DocumentViewResponse<any, EitherPayload>> {
  const viewParams: DocumentViewParams = params
    ? { ...params, reduce: false }
    : { reduce: false };
    const designDocName = "_design/" + datumView.name;
  try {
    return await db.view(designDocName, "default", viewParams);
  } catch (error) {
    if (isCouchDbError(error) && error.error === "not_found") {
      throw new DatumViewMissingError();
    } else {
      throw error;
    }
  }
}

export default viewMap;
