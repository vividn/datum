import {
  DatumView,
  StringifiedDatumView,
  ViewDocument,
} from "./DatumView";
import { datumViewToViewPayload } from "./datumViewToViewPayload";
import { addDoc, ConflictStrategyNames } from "../documentControl/addDoc";
import { BaseDocControlArgs } from "../documentControl/base";

type InsertDatumViewArgs = {
  datumView: DatumView<any, any, any, any> | StringifiedDatumView;
  conflictStrategy?: Extract<
    ConflictStrategyNames,
    "overwrite" | "useOld" | "fail"
  >;
} & BaseDocControlArgs;

export async function insertDatumView({
  db,
  datumView,
  outputArgs,
  conflictStrategy = "overwrite",
}: InsertDatumViewArgs): Promise<ViewDocument> {
  const viewPayload = datumViewToViewPayload(datumView);
  const newDesignDoc = (await addDoc({
    db,
    payload: viewPayload,
    outputArgs: outputArgs,
    conflictStrategy,
  })) as ViewDocument;
  return newDesignDoc;
}
