import {
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { DatumView, StringifiedDatumView } from "./DatumView";
import { DatumViewMissingError, isCouchDbError } from "../errors";

type ViewMapType = {
  db: PouchDB.Database<EitherPayload>;
  datumView:
    | DatumView<EitherDocument<any>, any, any, any>
    | StringifiedDatumView;
  params?: Omit<PouchDB.Query.Options<any, any>, "reduce">;
};
export async function viewMap({
  db,
  datumView,
  params,
}: ViewMapType): Promise<PouchDB.Query.Response<any>> {
  const viewParams = params ? { ...params, reduce: false } : { reduce: false };
  try {
    return await db.query(datumView.name, viewParams);
  } catch (error) {
    if (isCouchDbError(error) && error.name === "not_found") {
      throw new DatumViewMissingError(datumView.name);
    } else {
      throw error;
    }
  }
}

// TODO: Combine this functions and it's tests directly into mapCmd
