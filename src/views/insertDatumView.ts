import {
  DatumView,
  datumViewToViewPayload,
  StringifiedDatumView,
  ViewDocument,
} from "./DatumView";
import { addDoc } from "../documentControl/addDoc";
import { BaseDocControlArgs } from "../documentControl/base";

type InsertDatumViewArgs = {
  datumView: DatumView | StringifiedDatumView;
} & BaseDocControlArgs;

export async function insertDatumView({
  db,
  datumView,
  outputArgs,
}: InsertDatumViewArgs): Promise<ViewDocument> {
  const viewPayload = datumViewToViewPayload(datumView);
  const newDesignDoc = (await addDoc({
    db,
    payload: viewPayload,
    outputArgs: outputArgs,
    conflictStrategy: "overwrite",
  })) as ViewDocument;
  return newDesignDoc;
}
