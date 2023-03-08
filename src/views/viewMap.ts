import { DocumentScope, DocumentViewParams, DocumentViewResponse } from "nano";
import {
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { DatumView, StringifiedDatumView } from "./DatumView";
import { DatumViewMissingError, isCouchDbError } from "../errors";

type ViewMapType = {
  db: DocumentScope<EitherPayload>;
  datumView:
    | DatumView<EitherDocument<any>, any, any, any>
    | StringifiedDatumView;
  params?: Omit<DocumentViewParams, "reduce">;
};
export async function viewMap({
  db,
  datumView,
  params,
}: ViewMapType): Promise<DocumentViewResponse<any, EitherPayload>> {
  const viewParams: DocumentViewParams = params
    ? { ...params, reduce: false }
    : { reduce: false };
  try {
    return await db.view(datumView.name, "default", viewParams);
  } catch (error) {
    if (isCouchDbError(error) && error.error === "not_found") {
      throw new DatumViewMissingError(datumView.name, "default");
    } else {
      throw error;
    }
  }
}

// TODO: Combine this functions and it's tests directly into mapCmd
