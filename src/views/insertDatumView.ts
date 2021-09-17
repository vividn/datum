import {
  DatumView,
  datumViewToViewPayload, StringifiedDatumView,
  ViewDocument,
} from "./viewDocument";
import addDoc from "../documentControl/addDoc";
import { BaseDocControlArgs } from "../documentControl/base";

type InsertDatumViewArgs = {
  datumView: DatumView | StringifiedDatumView;
} & BaseDocControlArgs;

async function insertDatumView({
  db,
  datumView,
  show,
}: InsertDatumViewArgs): Promise<ViewDocument> {
  const viewPayload = datumViewToViewPayload(datumView);
  const newDesignDoc = (await addDoc({
    db,
    payload: viewPayload,
    show,
    conflictStrategy: "overwrite",
  })) as ViewDocument;
  return newDesignDoc;
}

export default insertDatumView;
