import {
  DataOrDesignPayload,
  DatumView,
  datumViewToViewPayload,
  ViewDocument,
  ViewPayload,
} from "./viewDocument";
import { DocumentScope } from "nano";
import addDoc from "../documentControl/addDoc";
import { BaseDocControlArgs } from "../documentControl/base";

type InsertDatumViewArgs = {
  datumView: DatumView;
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
